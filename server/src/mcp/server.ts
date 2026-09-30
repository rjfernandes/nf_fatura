import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import type { StatementService } from "../services/statementService.js";

const competence = z
  .string()
  .regex(/^\d{4}-(0[1-9]|1[0-2])$/, "Use o formato AAAA-MM")
  .describe(
    "Mês do extrato, no formato AAAA-MM (ex.: 2026-08). Pode ser o mês atual (até hoje) ou um anterior.",
  );

const brl = (cents: number) =>
  (cents / 100).toLocaleString("pt-BR", { style: "currency", currency: "BRL" });
const text = (t: string) => ({ content: [{ type: "text" as const, text: t }] });
const fail = (e: unknown) => ({
  isError: true,
  ...text(e instanceof Error ? e.message : String(e)),
});

export function buildMcpServer(
  statements: StatementService,
  defaultDir: string,
) {
  const server = new McpServer({ name: "nf-fatura-extrato", version: "1.0.0" });

  server.registerTool(
    "get_statement",
    {
      title: "Consultar extrato",
      description:
        "Busca no Banco Inter as movimentações de um mês e devolve totais (entradas, saídas, saldo do mês) e a lista de lançamentos. Não gera arquivo.",
      inputSchema: { competence },
      annotations: { readOnlyHint: true, openWorldHint: true },
    },
    async ({ competence }) => {
      try {
        const s = await statements.summary(competence);
        const lines = s.entries.map(
          (e) =>
            `${e.date} ${e.operation === "D" ? "-" : "+"}${brl(e.amountCents)} ${e.title}${e.description ? `: ${e.description}` : ""}`,
        );
        return text(
          [
            `Extrato ${s.from} a ${s.to}`,
            `Entradas: ${brl(s.totals.credits)} | Saídas: ${brl(s.totals.debits)} | Saldo do mês: ${brl(s.totals.net)}`,
            `${s.entries.length} lançamento(s)`,
            ...lines,
          ].join("\n"),
        );
      } catch (e) {
        return fail(e);
      }
    },
  );

  server.registerTool(
    "export_statement",
    {
      title: "Gerar extrato em PDF e/ou OFX",
      description:
        "Gera o extrato de um mês nos mesmos formatos exportados pelo Banco Inter (PDF e OFX 1.02) e salva os arquivos em disco. Devolve o caminho de cada arquivo.",
      inputSchema: {
        competence,
        formats: z
          .array(z.enum(["pdf", "ofx"]))
          .min(1)
          .default(["pdf", "ofx"])
          .describe("Formatos a gerar. Padrão: pdf e ofx."),
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
        openWorldHint: true,
      },
    },
    async ({ competence, formats, output_dir }) => {
      try {
        const dir = resolve(output_dir ?? defaultDir);
        await mkdir(dir, { recursive: true });
        const paths: string[] = [];
        for (const format of new Set(formats)) {
          const file = await statements.export(competence, format);
          const path = resolve(dir, file.fileName);
          await writeFile(path, file.content);
          paths.push(path);
        }
        return text(`Arquivos gerados:\n${paths.join("\n")}`);
      } catch (e) {
        return fail(e);
      }
    },
  );

  return server;
}
