import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import z from "zod";
import { McpDefs } from "./types.js";
import { competence, fail, text } from "./utils.js";

export function exportStatement({ server, statements, defaultDir }: McpDefs) {
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
}
