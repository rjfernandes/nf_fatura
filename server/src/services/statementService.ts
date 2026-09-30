import type {
  StatementEntry,
  StatementProvider,
} from "../integrations/types.js";
import { isFutureMonth, monthRangeUntilToday } from "../lib/period.js";
import { buildOfx, buildPdf, type AccountInfo } from "../lib/statementFiles.js";

export type StatementFormat = "pdf" | "ofx";

export interface StatementSummary {
  competence: string;
  from: string;
  to: string;
  entries: StatementEntry[];
  totals: { credits: number; debits: number; net: number }; // cents
}

export interface StatementFile {
  fileName: string;
  mime: string;
  content: Buffer;
}

export class StatementError extends Error {}

const dmy = (iso: string) => iso.split("-").reverse().join("-");

/**
 * Statement lookups and file exports, shared by the HTTP API and the MCP
 * server.
 */
export class StatementService {
  constructor(
    private provider: StatementProvider,
    private account: AccountInfo,
  ) {}

  private period(competence: string) {
    if (isFutureMonth(competence)) {
      throw new StatementError("Escolha o mês atual ou um anterior");
    }
    return monthRangeUntilToday(competence);
  }

  async summary(competence: string): Promise<StatementSummary> {
    const { from, to } = this.period(competence);
    const entries = await this.provider.fetch(from, to);
    const sum = (op: "C" | "D") =>
      entries
        .filter((e) => e.operation === op)
        .reduce((s, e) => s + e.amountCents, 0);
    const credits = sum("C");
    const debits = sum("D");
    return {
      competence,
      from,
      to,
      entries,
      totals: { credits, debits, net: credits - debits },
    };
  }

  /** Same layouts Banco Inter exports: PDF and OFX 1.02. */
  async export(
    competence: string,
    format: StatementFormat,
  ): Promise<StatementFile> {
    const { from, to } = this.period(competence);
    const [entries, balance] = await Promise.all([
      this.provider.fetch(from, to),
      this.provider.balance(to),
    ]);
    const data = {
      from,
      to,
      entries,
      available: balance.available,
      blocked: balance.blocked,
      closingCents: balance.available + balance.blocked,
    };
    const fileName = `Extrato-${dmy(from)}-a-${dmy(to)}-${format.toUpperCase()}.${format}`;
    return format === "ofx"
      ? {
          fileName,
          mime: "application/x-ofx",
          content: buildOfx(this.account, data),
        }
      : {
          fileName,
          mime: "application/pdf",
          content: await buildPdf(this.account, data),
        };
  }
}
