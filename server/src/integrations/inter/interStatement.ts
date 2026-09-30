import type {
  StatementBalance,
  StatementEntry,
  StatementProvider,
} from "../types.js";
import type { InterApi } from "./interApi.js";

const SCOPE = "extrato.read";
const PAGE_SIZE = 100;
const MAX_PAGES = 200;

interface InterTransaction {
  idTransacao?: string;
  dataInclusao?: string;
  dataTransacao?: string;
  tipoTransacao?: string;
  tipoOperacao?: string; // C crédito | D débito
  valor?: string | number;
  titulo?: string;
  descricao?: string;
  detalhes?: Record<string, unknown>;
}

interface InterStatementPage {
  totalPaginas?: number;
  ultimaPagina?: boolean;
  transacoes?: InterTransaction[];
}

const cents = (v: unknown) => Math.round(Number(v ?? 0) * 100);
const counterpartOf = (d: Record<string, unknown> = {}) =>
  String(
    d.nomePagador ??
      d.nomeRecebedor ??
      d.nomeOrigem ??
      d.nomeDestino ??
      d.nomeFavorecido ??
      "",
  );

export class InterStatementProvider implements StatementProvider {
  constructor(private inter: InterApi) {}

  async fetch(from: string, to: string): Promise<StatementEntry[]> {
    const entries: StatementEntry[] = [];
    for (let page = 0; page < MAX_PAGES; page++) {
      const qs = new URLSearchParams({
        dataInicio: from,
        dataFim: to,
        pagina: String(page),
        tamanhoPagina: String(PAGE_SIZE),
      });
      const res = await this.inter.call<InterStatementPage>(
        SCOPE,
        "GET",
        `/banking/v2/extrato/completo?${qs}`,
      );
      for (const t of res.transacoes ?? []) {
        entries.push({
          id: t.idTransacao ?? `${from}-${entries.length}`,
          date: t.dataTransacao ?? t.dataInclusao?.slice(0, 10) ?? from,
          at: t.dataInclusao ?? t.dataTransacao ?? from,
          type: t.tipoTransacao ?? "",
          operation: t.tipoOperacao === "D" ? "D" : "C",
          amountCents: cents(t.valor),
          title: t.titulo ?? "",
          description: t.descricao ?? "",
          counterpart: counterpartOf(t.detalhes),
        });
      }
      if (res.ultimaPagina || page + 1 >= (res.totalPaginas ?? 1)) {
        break;
      }
    }
    return entries.sort((a, b) => a.date.localeCompare(b.date));
  }

  async balance(date: string): Promise<StatementBalance> {
    const r = await this.inter.call<Record<string, number>>(
      SCOPE,
      "GET",
      `/banking/v2/saldo?dataSaldo=${date}`,
    );
    return {
      available: cents(r.disponivel),
      blocked:
        cents(r.bloqueadoCheque) +
        cents(r.bloqueadoJudicialmente) +
        cents(r.bloqueadoAdministrativo),
    };
  }
}
