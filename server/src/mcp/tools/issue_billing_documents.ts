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

export function issueBillingDocuments({
  server,
  billings,
  db,
  defaultDir,
}: McpDefs) {
  server.registerTool(
    "issue_billing_documents",
    {
      title: "Gerar boleto e/ou nota fiscal",
      description:
        "EMITE de verdade (Banco Inter / NFS-e Nacional) o boleto e/ou a nota fiscal de uma empresa em uma competência, usando o valor recorrente cadastrado, e salva os PDFs. Documentos que já existem não são emitidos de novo. Use apenas depois que o usuário confirmar o que gerar (boleto, nota ou ambos) e, para boleto, o vencimento.",
      inputSchema: {
        company: z
          .string()
          .trim()
          .min(1)
          .describe(
            "Empresa: parte do nome, apelido ou CNPJ (com ou sem pontuação).",
          ),
        competence,
        documents: documentsInput,
        due_date: z
          .string()
          .regex(/^\d{4}-\d{2}-\d{2}$/, "Use o formato AAAA-MM-DD")
          .optional()
          .describe(
            "Vencimento do boleto (AAAA-MM-DD). Obrigatório para boleto.",
          ),
        output_dir: z
          .string()
          .optional()
          .describe('Pasta de destino. Padrão: a pasta "exports" do servidor.'),
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: true,
      },
    },
    async ({ company, competence, documents, due_date, output_dir }) => {
      try {
        const kinds = new Set(documents);
        if (kinds.has("slip") && !due_date) {
          throw new Error("Informe due_date (AAAA-MM-DD) para gerar o boleto.");
        }
        const customer = await findCustomer(db, company);
        const b = await billings.issueDocuments({
          customerId: customer.id,
          competence,
          dueDate: due_date ?? new Date().toISOString().slice(0, 10),
          nfse: kinds.has("nfse"),
          slip: kinds.has("slip"),
        });

        const dir = output_dir ?? defaultDir;
        const lines = [
          `Empresa: ${customer.name} (${customer.cnpj})`,
          `Valor: ${brl(b.amountCents)}`,
        ];
        if (b.status === "FAILED") {
          lines.push(`Falha na emissão: ${b.error}`);
        }
        if (kinds.has("slip")) {
          lines.push(
            b.bankSlip
              ? `Boleto gerado, vencimento ${b.dueDate}. Linha digitável: ${b.bankSlip.linhaDigitavel ?? "indisponível"}`
              : `${label.slip} NÃO foi gerado.`,
          );
          if (b.bankSlip?.pdf) {
            lines.push(
              `Arquivo do boleto: ${await savePdf(dir, "Boleto", customer.name, competence, b.bankSlip.pdf)}`,
            );
          }
        }
        if (kinds.has("nfse")) {
          const pdf = b.invoice ? await billings.nfsePdf(b.id) : null;
          lines.push(
            b.invoice
              ? "Nota fiscal emitida."
              : `${label.nfse} NÃO foi emitida.`,
          );
          if (pdf) {
            lines.push(
              `Arquivo da nota fiscal: ${await savePdf(dir, "NFSe", customer.name, competence, pdf)}`,
            );
          }
        }
        return text(lines.join("\n"));
      } catch (e) {
        return fail(e);
      }
    },
  );
}
