import { McpDefs } from "./types.js";
import { brl, competence, fail, text } from "./utils.js";

export function getStatement({ server, statements, defaultDir }: McpDefs) {
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
}
