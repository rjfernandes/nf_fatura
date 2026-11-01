use crate::integrations::types::StatementEntry;
use crate::pdf::canvas::{Align, Font, Pdf, TextOpts};
use crate::util::brl_number;
use chrono::{Datelike, NaiveDateTime, Timelike};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct AccountInfo {
    pub company_name: String,
    pub cnpj: String,    // digits or formatted
    pub branch: String,  // e.g. 0001-9
    pub account: String, // digits, check digit included (e.g. 143906330)
}

pub struct StatementFileData {
    pub from: String, // YYYY-MM-DD
    pub to: String,
    pub entries: Vec<StatementEntry>,
    // total balance (available + blocked) at the end of the period
    pub closing_cents: i64,
    pub available: i64,
    pub blocked: i64,
}

pub struct EntryWithBalance {
    pub entry: StatementEntry,
    pub balance_cents: i64, // balance right after this entry
}

const BANK_ID: &str = "077";
const BANK_ORG: &str = "Banco Intermedium S/A";

fn signed(e: &StatementEntry) -> i64 {
    if e.operation == "D" {
        -e.amount_cents
    } else {
        e.amount_cents
    }
}

/// Chronological order (date, then bank timestamp, then arrival order) with the
/// running balance after each entry.
pub fn with_balances(entries: &[StatementEntry], closing_cents: i64) -> Vec<EntryWithBalance> {
    let mut ordered: Vec<&StatementEntry> = entries.iter().collect();
    // Stable, so arrival order breaks ties.
    ordered.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.at.cmp(&b.at)));
    let mut balance = closing_cents;
    let mut out: Vec<EntryWithBalance> = ordered
        .iter()
        .rev()
        .map(|e| {
            let row = EntryWithBalance {
                entry: (*e).clone(),
                balance_cents: balance,
            };
            balance -= signed(e);
            row
        })
        .collect();
    out.reverse();
    out
}

fn memo_of(e: &StatementEntry) -> String {
    if e.description.is_empty() {
        e.title.clone()
    } else {
        format!("{}: \"{}\"", e.title, e.description)
    }
}

fn title_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut start = true;
    for c in s.to_lowercase().chars() {
        if start {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
        start = c.is_whitespace();
    }
    out
}

fn compact(iso: &str) -> String {
    iso.replace('-', "")
}

fn amount(cents: i64) -> String {
    format!(
        "{}{}.{:02}",
        if cents < 0 { "-" } else { "" },
        cents.abs() / 100,
        cents.abs() % 100
    )
}

fn sgml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// OFX 1.02 (SGML) in the same layout Banco Inter exports: newest first, FITID =
/// date + bank id + per-day counter.
pub fn build_ofx(acc: &AccountInfo, d: &StatementFileData, generated_at: NaiveDateTime) -> Vec<u8> {
    let mut newest_first = with_balances(&d.entries, d.closing_cents);
    newest_first.reverse();
    let mut per_day: HashMap<String, u32> = HashMap::new();
    let mut lines: Vec<String> = [
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
    ]
    .map(String::from)
    .to_vec();
    lines.push(format!(
        "<DTSERVER>{}</DTSERVER>",
        generated_at.format("%Y%m%d")
    ));
    lines.extend([
        "<LANGUAGE>POR</LANGUAGE>".to_string(),
        "<FI>".into(),
        format!("<ORG>{BANK_ORG}</ORG>"),
        format!("<FID>{BANK_ID}</FID>"),
        "</FI>".into(),
        "</SONRS>".into(),
        "</SIGNONMSGSRSV1>".into(),
        "<BANKMSGSRSV1>".into(),
        "<STMTTRNRS>".into(),
        "<TRNUID>1001</TRNUID>".into(),
        "<STATUS>".into(),
        "<CODE>0</CODE>".into(),
        "<SEVERITY>INFO</SEVERITY>".into(),
        "</STATUS>".into(),
        "<STMTRS>".into(),
        "<CURDEF>BRL</CURDEF>".into(),
        "<BANKACCTFROM>".into(),
        format!("<BANKID>{BANK_ID}</BANKID>"),
        format!("<BRANCHID>{}</BRANCHID>", acc.branch),
        format!("<ACCTID>{}</ACCTID>", acc.account),
        "<ACCTTYPE>CHECKING</ACCTTYPE>".into(),
        "</BANKACCTFROM>".into(),
        "<BANKTRANLIST>".into(),
        format!("<DTSTART>{}</DTSTART>", compact(&d.from)),
        format!("<DTEND>{}</DTEND>", compact(&d.to)),
    ]);
    for row in &newest_first {
        let e = &row.entry;
        let n = per_day.entry(e.date.clone()).or_insert(0);
        *n += 1;
        let name = if e.kind.to_uppercase().contains("PIX") && !e.counterpart.is_empty() {
            title_case(&e.counterpart)
        } else if !e.counterpart.is_empty() {
            e.counterpart.clone()
        } else {
            e.title.clone()
        };
        lines.extend([
            "<STMTTRN>".to_string(),
            format!(
                "<TRNTYPE>{}</TRNTYPE>",
                if e.operation == "C" {
                    "CREDIT"
                } else {
                    "PAYMENT"
                }
            ),
            format!("<DTPOSTED>{}</DTPOSTED>", compact(&e.date)),
            format!("<TRNAMT>{}</TRNAMT>", amount(signed(e))),
            format!("<FITID>{}{BANK_ID}{n}</FITID>", compact(&e.date)),
            format!("<CHECKNUM>{BANK_ID}</CHECKNUM>"),
            format!("<REFNUM>{BANK_ID}</REFNUM>"),
            format!("<MEMO>{}</MEMO>", sgml(&memo_of(e))),
            format!("<NAME>{}</NAME>", sgml(&name)),
            "</STMTTRN>".into(),
        ]);
    }
    lines.extend([
        "</BANKTRANLIST>".to_string(),
        "<LEDGERBAL>".into(),
        format!("<BALAMT>{}</BALAMT>", amount(d.closing_cents)),
        format!("<DTASOF>{}</DTASOF>", compact(&d.to)),
        "</LEDGERBAL>".into(),
        "</STMTRS>".into(),
        "</STMTTRNRS>".into(),
        "</BANKMSGSRSV1>".into(),
        "</OFX>".into(),
        String::new(),
    ]);
    // The header declares CHARSET 1252: latin1 keeps accented characters intact.
    lines
        .join("\r\n")
        .chars()
        .map(|c| {
            if (c as u32) < 256 {
                c as u32 as u8
            } else {
                b'?'
            }
        })
        .collect()
}

// ------------------------------------------------------------------------- PDF

const MONTHS: [&str; 12] = [
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

fn brl(cents: i64) -> String {
    format!(
        "{}R$ {}",
        if cents < 0 { "-" } else { "" },
        brl_number(cents)
    )
}

fn day_label(iso: &str) -> String {
    let mut p = iso.split('-').map(|v| v.parse::<usize>().unwrap_or(1));
    let (y, m, d) = (
        p.next().unwrap_or(0),
        p.next().unwrap_or(1),
        p.next().unwrap_or(1),
    );
    format!("{d} de {} de {y}", MONTHS[(m.clamp(1, 12)) - 1])
}

fn dmy(iso: &str) -> String {
    iso.split('-').rev().collect::<Vec<_>>().join("/")
}

fn format_cnpj(v: &str) -> String {
    let d: String = v.chars().filter(char::is_ascii_digit).collect();
    if d.len() == 14 {
        format!(
            "{}.{}.{}/{}-{}",
            &d[..2],
            &d[2..5],
            &d[5..8],
            &d[8..12],
            &d[12..]
        )
    } else {
        d
    }
}

fn format_account(a: &str) -> String {
    if a.len() > 1 && !a.contains('-') {
        format!("{}-{}", &a[..a.len() - 1], &a[a.len() - 1..])
    } else {
        a.to_string()
    }
}

const L: f64 = 40.0;
const W: f64 = 515.0;
const R: f64 = L + W;
const COL_VALUE: (f64, f64) = (360.0, 90.0);
const COL_BALANCE: (f64, f64) = (455.0, 100.0);
const ROW: f64 = 24.0;
const BOTTOM: f64 = 800.0;
const GRAY: &str = "#6b7280";
const INK: &str = "#111827";

pub fn build_pdf(acc: &AccountInfo, d: &StatementFileData, generated_at: NaiveDateTime) -> Vec<u8> {
    let mut doc = Pdf::new(Some(&format!("Extrato {} a {}", dmy(&d.from), dmy(&d.to))));
    let one = TextOpts::default();
    let right = |w: f64| TextOpts::width(w).align(Align::Right);
    let line = |doc: &mut Pdf, y: f64, color: &str| {
        doc.stroke_color(color).line_width(0.7).line(L, y, R, y);
    };

    // Header
    doc.font(Font::Regular, 8.0).fill_color(GRAY).text(
        &format!(
            "Solicitado em: {:02}/{:02}/{} - {:02}h{:02}",
            generated_at.day(),
            generated_at.month(),
            generated_at.year(),
            generated_at.hour(),
            generated_at.minute()
        ),
        L,
        42.0,
        right(W),
    );
    doc.font(Font::Bold, 20.0)
        .fill_color(INK)
        .text("Extrato", L, 40.0, one);
    doc.font(Font::Bold, 10.0)
        .text(&acc.company_name, L, 82.0, one);
    doc.font(Font::Regular, 9.0).text(
        &format!(
            "CPF/CNPJ: {}, Instituição: Banco Inter, Agência: {}, Conta: {}",
            format_cnpj(&acc.cnpj),
            acc.branch,
            format_account(&acc.account)
        ),
        L,
        100.0,
        one,
    );
    doc.text(
        &format!("Período: {} a {}", dmy(&d.from), dmy(&d.to)),
        L,
        118.0,
        one,
    );
    line(&mut doc, 140.0, "#d1d5db");

    // Balances
    let mut bal = |x: f64, label: &str, value: &str, note: Option<&str>| {
        doc.font(Font::Regular, 8.0)
            .fill_color(GRAY)
            .text(label, x, 152.0, one);
        doc.font(Font::Bold, 10.0)
            .fill_color(INK)
            .text(value, x, 164.0, one);
        if let Some(note) = note {
            doc.font(Font::Regular, 6.0)
                .fill_color(GRAY)
                .text(note, x, 178.0, one);
        }
    };
    bal(
        L,
        "Saldo total",
        &brl(d.closing_cents),
        Some("(bloqueado + disponível)"),
    );
    bal(L + 110.0, "Saldo disponível:", &brl(d.available), None);
    bal(L + 220.0, "Saldo bloqueado:", &brl(d.blocked), None);
    line(&mut doc, 196.0, "#d1d5db");

    // Transactions grouped by day
    let entries = with_balances(&d.entries, d.closing_cents);
    let mut days: Vec<&str> = Vec::new();
    for e in &entries {
        if !days.contains(&e.entry.date.as_str()) {
            days.push(&e.entry.date);
        }
    }
    let mut y = 220.0;
    let mut first_group = true;

    for day in days {
        let rows: Vec<&EntryWithBalance> = entries.iter().filter(|e| e.entry.date == day).collect();
        if y + 30.0 + ROW > BOTTOM {
            doc.add_page();
            y = 40.0;
        }
        let day_balance = rows[rows.len() - 1].balance_cents;
        doc.font(Font::Bold, 9.0).fill_color(INK);
        let x = doc.text(&day_label(day), L + 4.0, y, one);
        let x = doc
            .font(Font::Regular, 9.0)
            .text("  Saldo do dia: ", x, y, one);
        doc.font(Font::Bold, 9.0).text(&brl(day_balance), x, y, one);
        if first_group {
            doc.font(Font::Bold, 8.0).fill_color(GRAY);
            doc.text("Valor", COL_VALUE.0, y + 1.0, right(COL_VALUE.1));
            doc.text(
                "Saldo por transação",
                COL_BALANCE.0 - 30.0,
                y + 1.0,
                right(COL_BALANCE.1 + 30.0),
            );
            first_group = false;
        }
        y += 18.0;
        line(&mut doc, y, "#9ca3af");
        y += 4.0;
        for (i, row) in rows.iter().enumerate() {
            if y + ROW > BOTTOM {
                doc.add_page();
                y = 40.0;
            }
            if i % 2 == 1 {
                doc.fill_rect(L, y, W, ROW, "#f3f4f6");
            }
            doc.font(Font::Regular, 9.0).fill_color(INK).text(
                &memo_of(&row.entry),
                L + 6.0,
                y + 8.0,
                TextOpts::width(310.0).clamp(12.0),
            );
            doc.font(Font::Bold, 9.0).text(
                &brl(signed(&row.entry)),
                COL_VALUE.0,
                y + 8.0,
                right(COL_VALUE.1),
            );
            doc.text(
                &brl(row.balance_cents),
                COL_BALANCE.0,
                y + 8.0,
                right(COL_BALANCE.1 - 4.0),
            );
            y += ROW;
        }
        y += 14.0;
    }

    if entries.is_empty() {
        doc.font(Font::Regular, 10.0).fill_color(GRAY).text(
            "Nenhuma movimentação no período.",
            L + 4.0,
            y,
            one,
        );
    }
    doc.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn acc() -> AccountInfo {
        AccountInfo {
            company_name: "MRF INFORMATICA LTDA".into(),
            cnpj: "03894412000100".into(),
            branch: "0001-9".into(),
            account: "143906330".into(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn e(
        date: &str,
        time: &str,
        operation: &str,
        amount_cents: i64,
        kind: &str,
        title: &str,
        description: &str,
        counterpart: &str,
    ) -> StatementEntry {
        StatementEntry {
            id: format!("{date}{time}"),
            date: date.into(),
            at: format!("{date}T{time}"),
            kind: kind.into(),
            operation: operation.into(),
            amount_cents,
            title: title.into(),
            description: description.into(),
            counterpart: counterpart.into(),
        }
    }

    // August 2026 statement exported by Banco Inter (arrival order deliberately
    // shuffled).
    fn entries() -> Vec<StatementEntry> {
        vec![
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
        ]
    }

    fn data(entries: Vec<StatementEntry>, closing: i64) -> StatementFileData {
        StatementFileData {
            from: "2026-08-01".into(),
            to: "2026-08-31".into(),
            entries,
            closing_cents: closing,
            available: closing,
            blocked: 0,
        }
    }

    fn at(h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 8, 31)
            .unwrap()
            .and_hms_opt(h, m, 0)
            .unwrap()
    }

    fn latin1(bytes: &[u8]) -> String {
        bytes.iter().map(|b| *b as char).collect()
    }

    #[test]
    fn rebuilds_the_running_balance_backwards_from_the_closing_balance() {
        let balances: Vec<i64> = with_balances(&entries(), 787)
            .iter()
            .map(|x| x.balance_cents)
            .collect();
        assert_eq!(
            balances,
            [2426008, 195008, 75008, 212793, 225793, 207962, 787]
        );
    }

    #[test]
    fn writes_the_ofx_in_the_layout_banco_inter_exports() {
        let ofx = latin1(&build_ofx(&acc(), &data(entries(), 787), at(0, 0)));
        assert!(ofx.starts_with("OFXHEADER:100\r\nDATA:OFXSGML\r\n"));
        assert!(ofx.contains("<DTSERVER>20260831</DTSERVER>"));
        assert!(ofx.contains("<BRANCHID>0001-9</BRANCHID>\r\n<ACCTID>143906330</ACCTID>"));
        // newest first; the per-day FITID counter also runs newest first
        let fitids: Vec<&str> = ofx
            .split("<FITID>")
            .skip(1)
            .map(|s| s.split('<').next().unwrap())
            .collect();
        assert_eq!(
            fitids,
            [
                "202608200771",
                "202608200772",
                "202608060771",
                "202608050771",
                "202608050772",
                "202608040771",
                "202608030771",
            ]
        );
        assert!(ofx.contains("<TRNAMT>-2071.75</TRNAMT>"));
        assert!(ofx.contains("<TRNAMT>130.00</TRNAMT>"));
        assert!(ofx.contains(
            "<MEMO>Pix recebido: \"Cp :60746948-AGUA MINERAL CASCATAI LTDA\"</MEMO>\r\n<NAME>Agua Mineral Cascatai Ltda</NAME>"
        ));
        assert!(ofx.contains("<MEMO>SIMPLES NACIONAL</MEMO>\r\n<NAME>Simples Nacional</NAME>"));
        // only Pix names are title-cased
        assert!(ofx.contains("<NAME>CAMIM ANCHIETA</NAME>"));
        assert!(ofx.contains("<LEDGERBAL>\r\n<BALAMT>7.87</BALAMT>\r\n<DTASOF>20260831</DTASOF>"));
    }

    #[test]
    fn escapes_sgml_reserved_characters_in_text_fields() {
        let one = vec![e(
            "2026-08-01",
            "08:00:00",
            "C",
            100,
            "PIX",
            "Pix recebido",
            "A & B <x>",
            "A & B",
        )];
        let ofx = latin1(&build_ofx(&acc(), &data(one, 100), at(0, 0)));
        assert!(ofx.contains("A &amp; B &lt;x&gt;"));
    }

    #[test]
    fn produces_a_pdf() {
        let pdf = build_pdf(&acc(), &data(entries(), 787), at(11, 18));
        assert_eq!(&pdf[..5], b"%PDF-");
        assert!(pdf.len() > 1000);
    }
}
