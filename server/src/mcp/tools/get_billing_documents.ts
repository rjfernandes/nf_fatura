import z from "zod";
import { McpDefs } from "./types.js";
import {
  brl,
  competence,
  documentsInput,
  fail,
  findCustomer,
  label,
  savePdf,
  text,
} from "./utils.js";

export function getBillingDocuments({
  server,
  billings,
  db,
  defaultDir,
}: McpDefs) {
  server.registerTool(
    "get_billing_documents",
    {
      title: "Obter boleto e/ou nota fiscal",
      description:
        "Localiza o boleto e/ou a nota fiscal (NFS-e) já emitidos para uma empresa em uma competência, salva os PDFs em disco e devolve os caminhos (e a linha digitável do boleto). Se algum documento não existir, NÃO emite nada: informa o que falta — pergunte ao usuário se deseja gerar o boleto, a nota ou ambos e, só após a confirmação, use issue_billing_documents.",
      inputSchema: {
        company: z
          .string()
          .trim()
          .min(1)
          .describe(
            "Empresa: parte do nome, apelido ou CNPJ (com ou sem pontuação).",
          ),
        competence: competence.describe(
          "Competência do faturamento, no formato AAAA-MM (ex.: 2026-08).",
        ),
        documents: documentsInput,
        output_dir: z
          .string()
          .optional()
          .describe(
            'Pasta de destino. Padrão: a pasta "exports" do servidor (criada se não existir).',
          ),
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ company, competence, documents, output_dir }) => {
      try {
        const customer = await findCustomer(db, company);
        const billing = await db.billing.findFirst({
          where: { customerId: customer.id, competence },
          include: { bankSlip: true, invoice: { select: { id: true } } },
        });
        const dir = output_dir ?? defaultDir;
        const lines = [`Empresa: ${customer.name} (${customer.cnpj})`];
        const missing: ("slip" | "nfse")[] = [];

        for (const kind of new Set(documents)) {
          if (kind === "slip") {
            const slip = billing?.bankSlip;
            if (!slip) {
              missing.push(kind);
              continue;
            }
            lines.push(
              `Boleto: ${brl(billing.amountCents)}, vencimento ${billing.dueDate}, situação ${slip.status ?? "desconhecida"}`,
              `Linha digitável: ${slip.linhaDigitavel ?? "indisponível"}`,
              slip.pdf
                ? `Arquivo do boleto: ${await savePdf(dir, "Boleto", customer.name, competence, slip.pdf)}`
                : "PDF do boleto indisponível.",
            );
          } else {
            if (!billing?.invoice) {
              missing.push(kind);
              continue;
            }
            const pdf = await billings.nfsePdf(billing.id);
            lines.push(
              pdf
                ? `Arquivo da nota fiscal: ${await savePdf(dir, "NFSe", customer.name, competence, pdf)}`
                : "Nota fiscal emitida, mas o PDF está indisponível.",
            );
          }
        }

        if (missing.length) {
          lines.push(
            `Não existe ${missing.map((k) => label[k]).join(" nem ")} em ${competence} para esta empresa.`,
            billing?.status === "FAILED" && billing.error
              ? `A última tentativa de emissão falhou: ${billing.error}`
              : "",
            "Pergunte ao usuário se deseja gerar o boleto, a nota ou ambos (para o boleto, confirme também o vencimento).",
          );
        }
        return text(lines.filter(Boolean).join("\n"));
      } catch (e) {
        return fail(e);
      }
    },
  );
}
