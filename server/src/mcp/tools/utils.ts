import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import type { PrismaClient } from "../../generated/prisma/client.js";
import z from "zod";

export const competence = z
  .string()
  .regex(/^\d{4}-(0[1-9]|1[0-2])$/, "Use o formato AAAA-MM")
  .describe(
    "Mês do extrato, no formato AAAA-MM (ex.: 2026-08). Pode ser o mês atual (até hoje) ou um anterior.",
  );

export const brl = (cents: number) =>
  (cents / 100).toLocaleString("pt-BR", { style: "currency", currency: "BRL" });

export const text = (t: string) => ({
  content: [{ type: "text" as const, text: t }],
});

export const fail = (e: unknown) => ({
  isError: true,
  ...text(e instanceof Error ? e.message : String(e)),
});

export const documentKinds = z.enum(["slip", "nfse"]);
export const documentsInput = z
  .array(documentKinds)
  .min(1)
  .default(["slip", "nfse"])
  .describe(
    'Documentos: "slip" (boleto), "nfse" (nota fiscal) ou ambos. Padrão: ambos.',
  );
export const label = { slip: "boleto", nfse: "nota fiscal (NFS-e)" } as const;

/** Finds exactly one customer by part of the name/nickname or by CNPJ. */
export async function findCustomer(db: PrismaClient, company: string) {
  const cnpj = company.replace(/\D/g, "");
  const found = await db.customer.findMany({
    where: {
      OR: [
        { name: { contains: company } },
        { nickName: { contains: company } },
        ...(cnpj ? [{ cnpj: { contains: cnpj } }] : []),
      ],
    },
    select: { id: true, name: true, cnpj: true },
  });
  if (found.length === 0) {
    throw new Error(`Nenhuma empresa encontrada para "${company}".`);
  }
  if (found.length > 1) {
    throw new Error(
      `Mais de uma empresa encontrada, refine a busca:\n${found.map((c) => `- ${c.name} (${c.cnpj})`).join("\n")}`,
    );
  }
  return found[0];
}

export async function savePdf(
  dir: string,
  prefix: string,
  customerName: string,
  competence: string,
  pdf: Uint8Array,
) {
  await mkdir(dir, { recursive: true });
  const safe = customerName.replace(/[^\p{L}\p{N}]+/gu, "-");
  const path = resolve(dir, `${prefix}-${safe}-${competence}.pdf`);
  await writeFile(path, pdf);
  return path;
}
