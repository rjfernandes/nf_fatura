import PDFDocument from "pdfkit";
import type { StatementEntry } from "../integrations/types.js";

export interface AccountInfo {
  companyName: string;
  cnpj: string; // digits or formatted
  branch: string; // e.g. 0001-9
  account: string; // digits, check digit included (e.g. 143906330)
}

export interface StatementFileData {
  from: string; // YYYY-MM-DD
  to: string;
  entries: StatementEntry[];
  // total balance (available + blocked) at the end of the period
  closingCents: number;
  available: number;
  blocked: number;
}

export interface EntryWithBalance extends StatementEntry {
  balanceCents: number; // balance right after this entry
}

const BANK_ID = "077";
const BANK_ORG = "Banco Intermedium S/A";
const signed = (e: StatementEntry) =>
  e.operation === "D" ? -e.amountCents : e.amountCents;

/**
 * Chronological order (date, then bank timestamp, then arrival order) with the
 * running balance after each entry.
 */
export function withBalances(
  entries: StatementEntry[],
  closingCents: number,
): EntryWithBalance[] {
  const ordered = entries
    .map((e, i) => ({ e, i }))
    .sort(
      (a, b) =>
        a.e.date.localeCompare(b.e.date) ||
        a.e.at.localeCompare(b.e.at) ||
        a.i - b.i,
    )
    .map((x) => x.e);
  const out: EntryWithBalance[] = new Array(ordered.length);
  let balance = closingCents;
  for (let i = ordered.length - 1; i >= 0; i--) {
    out[i] = { ...ordered[i], balanceCents: balance };
    balance -= signed(ordered[i]);
  }
  return out;
}

const memoOf = (e: StatementEntry) =>
  e.description ? `${e.title}: "${e.description}"` : e.title;

const titleCase = (s: string) =>
  s
    .toLowerCase()
    .replace(/(^|\s)(\S)/g, (_, sp: string, c: string) => sp + c.toUpperCase());

const compact = (iso: string) => iso.replaceAll("-", "");
const amount = (cents: number) => (cents / 100).toFixed(2);
const sgml = (s: string) =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/**
 * OFX 1.02 (SGML) in the same layout Banco Inter exports: newest first, FITID =
 * date + bank id + per-day counter.
 */
export function buildOfx(
  acc: AccountInfo,
  d: StatementFileData,
  generatedAt = new Date(),
): Buffer {
  const newestFirst = withBalances(d.entries, d.closingCents).reverse();
  const perDay = new Map<string, number>();
  const trns = newestFirst.map((e) => {
    const n = (perDay.get(e.date) ?? 0) + 1;
    perDay.set(e.date, n);
    const name =
      e.type.toUpperCase().includes("PIX") && e.counterpart
        ? titleCase(e.counterpart)
        : e.counterpart || e.title;
    return [
      "<STMTTRN>",
      `<TRNTYPE>${e.operation === "C" ? "CREDIT" : "PAYMENT"}</TRNTYPE>`,
      `<DTPOSTED>${compact(e.date)}</DTPOSTED>`,
      `<TRNAMT>${amount(signed(e))}</TRNAMT>`,
      `<FITID>${compact(e.date)}${BANK_ID}${n}</FITID>`,
      `<CHECKNUM>${BANK_ID}</CHECKNUM>`,
      `<REFNUM>${BANK_ID}</REFNUM>`,
      `<MEMO>${sgml(memoOf(e))}</MEMO>`,
      `<NAME>${sgml(name)}</NAME>`,
      "</STMTTRN>",
    ].join("\r\n");
  });
  const generated = `${generatedAt.getFullYear()}${String(generatedAt.getMonth() + 1).padStart(2, "0")}${String(generatedAt.getDate()).padStart(2, "0")}`;
  const text = [
    "OFXHEADER:100",
    "DATA:OFXSGML",
    "VERSION:102",
    "SECURITY:NONE",
    "ENCODING:USASCII",
    "CHARSET:1252",
    "COMPRESSION:NONE",
    "OLDFILEUID:NONE",
    "NEWFILEUID:NONE",
    "",
    "<OFX>",
    "<SIGNONMSGSRSV1>",
    "<SONRS>",
    "<STATUS>",
    "<CODE>0</CODE>",
    "<SEVERITY>INFO</SEVERITY>",
    "</STATUS>",
    `<DTSERVER>${generated}</DTSERVER>`,
    "<LANGUAGE>POR</LANGUAGE>",
    "<FI>",
    `<ORG>${BANK_ORG}</ORG>`,
    `<FID>${BANK_ID}</FID>`,
    "</FI>",
    "</SONRS>",
    "</SIGNONMSGSRSV1>",
    "<BANKMSGSRSV1>",
    "<STMTTRNRS>",
    "<TRNUID>1001</TRNUID>",
    "<STATUS>",
    "<CODE>0</CODE>",
    "<SEVERITY>INFO</SEVERITY>",
    "</STATUS>",
    "<STMTRS>",
    "<CURDEF>BRL</CURDEF>",
    "<BANKACCTFROM>",
    `<BANKID>${BANK_ID}</BANKID>`,
    `<BRANCHID>${acc.branch}</BRANCHID>`,
    `<ACCTID>${acc.account}</ACCTID>`,
    "<ACCTTYPE>CHECKING</ACCTTYPE>",
    "</BANKACCTFROM>",
    "<BANKTRANLIST>",
    `<DTSTART>${compact(d.from)}</DTSTART>`,
    `<DTEND>${compact(d.to)}</DTEND>`,
    ...trns,
    "</BANKTRANLIST>",
    "<LEDGERBAL>",
    `<BALAMT>${amount(d.closingCents)}</BALAMT>`,
    `<DTASOF>${compact(d.to)}</DTASOF>`,
    "</LEDGERBAL>",
    "</STMTRS>",
    "</STMTTRNRS>",
    "</BANKMSGSRSV1>",
    "</OFX>",
    "",
  ].join("\r\n");
  // The header declares CHARSET 1252: latin1 keeps accented characters intact.
  return Buffer.from(text, "latin1");
}

// ------------------------------------------------------------------------- PDF

const MONTHS = [
  "Janeiro",
  "Fevereiro",
  "Março",
  "Abril",
  "Maio",
  "Junho",
  "Julho",
  "Agosto",
  "Setembro",
  "Outubro",
  "Novembro",
  "Dezembro",
];
const brl = (cents: number) => {
  const abs = Math.abs(cents);
  const n = (abs / 100).toLocaleString("pt-BR", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
  return `${cents < 0 ? "-" : ""}R$ ${n}`;
};
const dayLabel = (iso: string) => {
  const [y, m, d] = iso.split("-").map(Number);
  return `${d} de ${MONTHS[m - 1]} de ${y}`;
};
const dmy = (iso: string) => iso.split("-").reverse().join("/");
const formatCnpj = (v: string) =>
  v
    .replace(/\D/g, "")
    .replace(/^(\d{2})(\d{3})(\d{3})(\d{4})(\d{2})$/, "$1.$2.$3/$4-$5");
const formatAccount = (a: string) =>
  a.length > 1 && !a.includes("-") ? `${a.slice(0, -1)}-${a.slice(-1)}` : a;

const L = 40;
const W = 515;
const R = L + W;
const COL_VALUE = { x: 360, w: 90 };
const COL_BALANCE = { x: 455, w: 100 };
const ROW = 24;
const BOTTOM = 800;

export function buildPdf(
  acc: AccountInfo,
  d: StatementFileData,
  generatedAt = new Date(),
): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    const doc = new PDFDocument({
      size: "A4",
      margin: 0,
      info: { Title: `Extrato ${dmy(d.from)} a ${dmy(d.to)}` },
    });
    const chunks: Buffer[] = [];
    doc.on("data", (c: Buffer) => chunks.push(c));
    doc.on("end", () => resolve(Buffer.concat(chunks)));
    doc.on("error", reject);

    const gray = "#6b7280";
    const line = (y: number, color = "#d1d5db") =>
      doc.moveTo(L, y).lineTo(R, y).strokeColor(color).lineWidth(0.7).stroke();
    const pad = (n: number) => String(n).padStart(2, "0");

    // Header
    doc
      .font("Helvetica")
      .fontSize(8)
      .fillColor(gray)
      .text(
        `Solicitado em: ${pad(generatedAt.getDate())}/${pad(generatedAt.getMonth() + 1)}/${generatedAt.getFullYear()} - ${pad(generatedAt.getHours())}h${pad(generatedAt.getMinutes())}`,
        L,
        42,
        { width: W, align: "right" },
      );
    doc
      .font("Helvetica-Bold")
      .fontSize(20)
      .fillColor("#111827")
      .text("Extrato", L, 40, { width: 300, lineBreak: false });
    doc
      .font("Helvetica-Bold")
      .fontSize(10)
      .fillColor("#111827")
      .text(acc.companyName, L, 82, { lineBreak: false });
    doc
      .font("Helvetica")
      .fontSize(9)
      .fillColor("#111827")
      .text(
        `CPF/CNPJ: ${formatCnpj(acc.cnpj)}, Instituição: Banco Inter, Agência: ${acc.branch}, Conta: ${formatAccount(acc.account)}`,
        L,
        100,
        { lineBreak: false },
      );
    doc.text(`Período: ${dmy(d.from)} a ${dmy(d.to)}`, L, 118, {
      lineBreak: false,
    });
    line(140);

    // Balances
    const bal = (x: number, label: string, value: string, note?: string) => {
      doc
        .font("Helvetica")
        .fontSize(8)
        .fillColor(gray)
        .text(label, x, 152, { lineBreak: false });
      doc
        .font("Helvetica-Bold")
        .fontSize(10)
        .fillColor("#111827")
        .text(value, x, 164, { lineBreak: false });
      if (note) {
        doc
          .font("Helvetica")
          .fontSize(6)
          .fillColor(gray)
          .text(note, x, 178, { lineBreak: false });
      }
    };
    bal(L, "Saldo total", brl(d.closingCents), "(bloqueado + disponível)");
    bal(L + 110, "Saldo disponível:", brl(d.available));
    bal(L + 220, "Saldo bloqueado:", brl(d.blocked));
    line(196);

    // Transactions grouped by day
    const entries = withBalances(d.entries, d.closingCents);
    const days = [...new Set(entries.map((e) => e.date))];
    let y = 220;
    let firstGroup = true;
    const newPage = () => {
      doc.addPage({ size: "A4", margin: 0 });
      y = 40;
    };

    for (const day of days) {
      const rows = entries.filter((e) => e.date === day);
      if (y + 30 + ROW > BOTTOM) {
        newPage();
      }
      const dayBalance = rows[rows.length - 1].balanceCents;
      doc
        .font("Helvetica-Bold")
        .fontSize(9)
        .fillColor("#111827")
        .text(dayLabel(day), L + 4, y, { continued: true, lineBreak: false });
      doc
        .font("Helvetica")
        .text("  Saldo do dia: ", { continued: true, lineBreak: false });
      doc.font("Helvetica-Bold").text(brl(dayBalance), { lineBreak: false });
      if (firstGroup) {
        doc.font("Helvetica-Bold").fontSize(8).fillColor(gray);
        doc.text("Valor", COL_VALUE.x, y + 1, {
          width: COL_VALUE.w,
          align: "right",
          lineBreak: false,
        });
        doc.text("Saldo por transação", COL_BALANCE.x - 30, y + 1, {
          width: COL_BALANCE.w + 30,
          align: "right",
          lineBreak: false,
        });
        firstGroup = false;
      }
      y += 18;
      line(y, "#9ca3af");
      y += 4;
      rows.forEach((e, i) => {
        if (y + ROW > BOTTOM) {
          newPage();
        }
        if (i % 2 === 1) {
          doc.rect(L, y, W, ROW).fillColor("#f3f4f6").fill();
        }
        doc
          .font("Helvetica")
          .fontSize(9)
          .fillColor("#111827")
          .text(memoOf(e), L + 6, y + 8, {
            width: 310,
            ellipsis: true,
            lineBreak: false,
            height: 12,
          });
        doc.font("Helvetica-Bold").text(brl(signed(e)), COL_VALUE.x, y + 8, {
          width: COL_VALUE.w,
          align: "right",
          lineBreak: false,
        });
        doc.text(brl(e.balanceCents), COL_BALANCE.x, y + 8, {
          width: COL_BALANCE.w - 4,
          align: "right",
          lineBreak: false,
        });
        y += ROW;
      });
      y += 14;
    }

    if (entries.length === 0) {
      doc
        .font("Helvetica")
        .fontSize(10)
        .fillColor(gray)
        .text("Nenhuma movimentação no período.", L + 4, y, {
          lineBreak: false,
        });
    }
    doc.end();
  });
}
