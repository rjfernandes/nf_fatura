import { describe, expect, it } from "vitest";
import type { StatementEntry } from "../integrations/types.js";
import {
  buildOfx,
  buildPdf,
  withBalances,
  type AccountInfo,
} from "./statementFiles.js";

const acc: AccountInfo = {
  companyName: "MRF INFORMATICA LTDA",
  cnpj: "03894412000100",
  branch: "0001-9",
  account: "143906330",
};

const e = (
  date: string,
  time: string,
  operation: "C" | "D",
  amountCents: number,
  type: string,
  title: string,
  description: string,
  counterpart: string,
): StatementEntry => ({
  id: `${date}${time}`,
  date,
  at: `${date}T${time}`,
  type,
  operation,
  amountCents,
  title,
  description,
  counterpart,
});

// August 2026 statement exported by Banco Inter (arrival order deliberately
// shuffled).
const entries = [
  e(
    "2026-08-20",
    "10:01:00",
    "D",
    207175,
    "IMPOSTO",
    "SIMPLES NACIONAL",
    "",
    "Simples Nacional",
  ),
  e(
    "2026-08-03",
    "09:00:00",
    "C",
    2425500,
    "BOLETO_COBRANCA",
    "Boleto de cobranca recebido",
    "112/90801485353",
    "CAMIM ANCHIETA",
  ),
  e(
    "2026-08-05",
    "11:00:00",
    "C",
    137785,
    "PIX",
    "Pix recebido",
    "Cp :60746948-AGUA MINERAL CASCATAI LTDA",
    "AGUA MINERAL CASCATAI LTDA",
  ),
  e(
    "2026-08-04",
    "09:00:00",
    "D",
    2231000,
    "PIX",
    "Pix enviado",
    "00019 140654674 ROBSON FERNANDES",
    "ROBSON DE JESUS FERNANDES",
  ),
  e(
    "2026-08-05",
    "10:00:00",
    "D",
    120000,
    "PIX",
    "Pix enviado",
    "Cp :60701190-Robson de Jesus Fernandes",
    "ROBSON DE JESUS FERNANDES",
  ),
  e(
    "2026-08-20",
    "10:00:00",
    "D",
    17831,
    "IMPOSTO",
    "DARF NUMERADO",
    "",
    "Darf Numerado",
  ),
  e(
    "2026-08-06",
    "09:00:00",
    "C",
    13000,
    "PIX",
    "Pix recebido",
    "00019 140654674 ROBSON FERNANDES",
    "ROBSON DE JESUS FERNANDES",
  ),
];

describe("statement files", () => {
  it("rebuilds the running balance backwards from the closing balance", () => {
    const balances = withBalances(entries, 787).map((x) => [
      x.title,
      x.balanceCents,
    ]);
    expect(balances.map(([, b]) => b)).toEqual([
      2426008, 195008, 75008, 212793, 225793, 207962, 787,
    ]);
  });

  it("writes the OFX in the layout Banco Inter exports", () => {
    const ofx = buildOfx(
      acc,
      {
        from: "2026-08-01",
        to: "2026-08-31",
        entries,
        closingCents: 787,
        available: 787,
        blocked: 0,
      },
      new Date(2026, 7, 31),
    ).toString("latin1");
    expect(ofx.startsWith("OFXHEADER:100\r\nDATA:OFXSGML\r\n")).toBe(true);
    expect(ofx).toContain("<DTSERVER>20260831</DTSERVER>");
    expect(ofx).toContain(
      "<BRANCHID>0001-9</BRANCHID>\r\n<ACCTID>143906330</ACCTID>",
    );
    // newest first; the per-day FITID counter also runs newest first
    const fitids = [...ofx.matchAll(/<FITID>(\d+)<\/FITID>/g)].map((m) => m[1]);
    expect(fitids).toEqual([
      "202608200771",
      "202608200772",
      "202608060771",
      "202608050771",
      "202608050772",
      "202608040771",
      "202608030771",
    ]);
    expect(ofx).toContain("<TRNAMT>-2071.75</TRNAMT>");
    expect(ofx).toContain("<TRNAMT>130.00</TRNAMT>");
    expect(ofx).toContain(
      '<MEMO>Pix recebido: "Cp :60746948-AGUA MINERAL CASCATAI LTDA"</MEMO>\r\n<NAME>Agua Mineral Cascatai Ltda</NAME>',
    );
    expect(ofx).toContain(
      "<MEMO>SIMPLES NACIONAL</MEMO>\r\n<NAME>Simples Nacional</NAME>",
    );
    // only Pix names are title-cased
    expect(ofx).toContain("<NAME>CAMIM ANCHIETA</NAME>");
    expect(ofx).toContain(
      "<LEDGERBAL>\r\n<BALAMT>7.87</BALAMT>\r\n<DTASOF>20260831</DTASOF>",
    );
  });

  it("escapes SGML-reserved characters in text fields", () => {
    const ofx = buildOfx(acc, {
      from: "2026-08-01",
      to: "2026-08-31",
      entries: [
        e(
          "2026-08-01",
          "08:00:00",
          "C",
          100,
          "PIX",
          "Pix recebido",
          "A & B <x>",
          "A & B",
        ),
      ],
      closingCents: 100,
      available: 100,
      blocked: 0,
    }).toString("latin1");
    expect(ofx).toContain("A &amp; B &lt;x&gt;");
  });

  it("produces a PDF", async () => {
    const pdf = await buildPdf(
      acc,
      {
        from: "2026-08-01",
        to: "2026-08-31",
        entries,
        closingCents: 787,
        available: 787,
        blocked: 0,
      },
      new Date(2026, 7, 31, 11, 18),
    );
    expect(pdf.subarray(0, 5).toString()).toBe("%PDF-");
    expect(pdf.length).toBeGreaterThan(1000);
  });
});
