use super::customers::{self, Customer};
use crate::error::{AppError, Result};
use crate::util::{new_id, now_iso};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BillingCustomer {
    pub name: String,
    pub nick_name: Option<String>,
    pub cnpj: String,
    pub integration: Option<String>,
    pub station: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BillingInvoice {
    pub number: Option<String>,
    pub access_key: Option<String>,
    pub source: String, // API | MANUAL | IMPORTED
    pub has_pdf: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BillingSlip {
    pub linha_digitavel: Option<String>,
    pub status: Option<String>,
    pub has_pdf: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BillingDelivery {
    pub status: String, // SENT | FAILED
    pub parts: String,  // NFSE | SLIP | BOTH
    pub error: Option<String>,
    pub sent_at: Option<String>,
}

/// A billing as the history shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BillingView {
    pub id: String,
    pub customer_id: String,
    pub competence: String, // YYYY-MM
    pub amount_cents: i64,
    pub due_date: String, // YYYY-MM-DD
    pub issue_nfse: bool,
    pub issue_slip: bool,
    pub status: String, // PENDING | NFSE_ISSUED | COMPLETED | FAILED
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub customer: BillingCustomer,
    pub invoice: Option<BillingInvoice>,
    pub bank_slip: Option<BillingSlip>,
    pub delivery: Option<BillingDelivery>,
}

// The PDF flags come without loading the bytes of each document. An invoice
// issued here has one even without the stored file: the DANFSe is rendered
// from its XML.
const VIEW: &str = "SELECT b.id, b.customerId, b.competence, b.amountCents, b.dueDate, \
    b.issueNfse, b.issueSlip, b.status, b.error, b.createdAt, b.updatedAt, \
    c.name, c.nickName, c.cnpj, c.integration, c.station, \
    i.id IS NOT NULL, i.number, i.accessKey, i.source, \
    (i.pdf IS NOT NULL OR (i.source <> 'MANUAL' AND i.xml IS NOT NULL)), \
    s.id IS NOT NULL, s.linhaDigitavel, s.status, s.pdf IS NOT NULL, \
    d.id IS NOT NULL, d.status, d.parts, d.error, d.sentAt \
    FROM Billing b \
    JOIN Customer c ON c.id = b.customerId \
    LEFT JOIN Invoice i ON i.billingId = b.id \
    LEFT JOIN BankSlip s ON s.billingId = b.id \
    LEFT JOIN Delivery d ON d.billingId = b.id";

fn view_from_row(r: &rusqlite::Row) -> rusqlite::Result<BillingView> {
    Ok(BillingView {
        id: r.get(0)?,
        customer_id: r.get(1)?,
        competence: r.get(2)?,
        amount_cents: r.get(3)?,
        due_date: r.get(4)?,
        issue_nfse: r.get(5)?,
        issue_slip: r.get(6)?,
        status: r.get(7)?,
        error: r.get(8)?,
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
        customer: BillingCustomer {
            name: r.get(11)?,
            nick_name: r.get(12)?,
            cnpj: r.get(13)?,
            integration: r.get(14)?,
            station: r.get(15)?,
        },
        invoice: if r.get(16)? {
            Some(BillingInvoice {
                number: r.get(17)?,
                access_key: r.get(18)?,
                source: r.get(19)?,
                has_pdf: r.get(20)?,
            })
        } else {
            None
        },
        bank_slip: if r.get(21)? {
            Some(BillingSlip {
                linha_digitavel: r.get(22)?,
                status: r.get(23)?,
                has_pdf: r.get(24)?,
            })
        } else {
            None
        },
        delivery: if r.get(25)? {
            Some(BillingDelivery {
                status: r.get(26)?,
                parts: r.get(27)?,
                error: r.get(28)?,
                sent_at: r.get(29)?,
            })
        } else {
            None
        },
    })
}

/// Latest 200 billings, newest competence first.
pub fn list(c: &Connection) -> Result<Vec<BillingView>> {
    let mut stmt = c.prepare(&format!(
        "{VIEW} ORDER BY b.competence DESC, b.createdAt DESC LIMIT 200"
    ))?;
    let rows = stmt.query_map([], view_from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn view(c: &Connection, id: &str) -> Result<BillingView> {
    c.query_row(&format!("{VIEW} WHERE b.id = ?1"), [id], view_from_row)
        .optional()?
        .ok_or_else(|| AppError::new(404, "Faturamento não encontrado"))
}

#[derive(Debug, Clone)]
pub struct BillingRow {
    pub id: String,
    pub customer_id: String,
    pub competence: String,
    pub amount_cents: i64,
    pub due_date: String,
    pub issue_nfse: bool,
    pub issue_slip: bool,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InvoiceMeta {
    pub source: String,
    pub has_stored_pdf: bool,
}

#[derive(Debug, Clone)]
pub struct SlipMeta {
    pub codigo_solicitacao: String,
    pub has_pdf: bool,
}

/// A billing with what the services decide on: its customer and which
/// documents and delivery it already has.
#[derive(Debug, Clone)]
pub struct BillingCtx {
    pub billing: BillingRow,
    pub customer: Customer,
    pub invoice: Option<InvoiceMeta>,
    pub slip: Option<SlipMeta>,
    pub delivery_status: Option<String>,
}

const ROW: &str =
    "id, customerId, competence, amountCents, dueDate, issueNfse, issueSlip, status, error";

fn row_from(r: &rusqlite::Row) -> rusqlite::Result<BillingRow> {
    Ok(BillingRow {
        id: r.get(0)?,
        customer_id: r.get(1)?,
        competence: r.get(2)?,
        amount_cents: r.get(3)?,
        due_date: r.get(4)?,
        issue_nfse: r.get(5)?,
        issue_slip: r.get(6)?,
        status: r.get(7)?,
        error: r.get(8)?,
    })
}

pub fn ctx(c: &Connection, id: &str) -> Result<Option<BillingCtx>> {
    let Some(billing) = c
        .query_row(
            &format!("SELECT {ROW} FROM Billing WHERE id = ?1"),
            [id],
            row_from,
        )
        .optional()?
    else {
        return Ok(None);
    };
    let customer = customers::find(c, &billing.customer_id)?
        .ok_or_else(|| AppError::internal("Cliente do faturamento não encontrado"))?;
    let invoice = c
        .query_row(
            "SELECT source, pdf IS NOT NULL FROM Invoice WHERE billingId = ?1",
            [id],
            |r| {
                Ok(InvoiceMeta {
                    source: r.get(0)?,
                    has_stored_pdf: r.get(1)?,
                })
            },
        )
        .optional()?;
    let slip = c
        .query_row(
            "SELECT codigoSolicitacao, pdf IS NOT NULL FROM BankSlip WHERE billingId = ?1",
            [id],
            |r| {
                Ok(SlipMeta {
                    codigo_solicitacao: r.get(0)?,
                    has_pdf: r.get(1)?,
                })
            },
        )
        .optional()?;
    let delivery_status = c
        .query_row(
            "SELECT status FROM Delivery WHERE billingId = ?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(Some(BillingCtx {
        billing,
        customer,
        invoice,
        slip,
        delivery_status,
    }))
}

/// Oldest billing of the customer's competence, optionally only one still
/// without the given document table ("Invoice" or "BankSlip").
pub fn find_for_competence(
    c: &Connection,
    customer_id: &str,
    competence: &str,
    without: Option<&str>,
) -> Result<Option<BillingRow>> {
    let missing = match without {
        Some(table) => {
            format!(" AND NOT EXISTS (SELECT 1 FROM {table} x WHERE x.billingId = Billing.id)")
        }
        None => String::new(),
    };
    Ok(c.query_row(
        &format!(
            "SELECT {ROW} FROM Billing WHERE customerId = ?1 AND competence = ?2{missing} \
             ORDER BY createdAt ASC LIMIT 1"
        ),
        [customer_id, competence],
        row_from,
    )
    .optional()?)
}

pub struct NewBilling<'a> {
    pub customer_id: &'a str,
    pub competence: &'a str,
    pub amount_cents: i64,
    pub due_date: &'a str,
    pub issue_nfse: bool,
    pub issue_slip: bool,
    pub status: &'a str,
}

pub fn create(c: &Connection, b: &NewBilling) -> Result<String> {
    let id = new_id();
    c.execute(
        "INSERT INTO Billing (id, customerId, competence, amountCents, dueDate, issueNfse, \
         issueSlip, status, createdAt, updatedAt) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
        params![
            id,
            b.customer_id,
            b.competence,
            b.amount_cents,
            b.due_date,
            b.issue_nfse,
            b.issue_slip,
            b.status,
            now_iso()
        ],
    )?;
    Ok(id)
}

pub fn set_status(c: &Connection, id: &str, status: &str, error: Option<&str>) -> Result<()> {
    c.execute(
        "UPDATE Billing SET status = ?2, error = ?3, updatedAt = ?4 WHERE id = ?1",
        params![id, status, error, now_iso()],
    )?;
    Ok(())
}

/// Which documents the billing asks for; None leaves the value as it is.
pub fn set_wanted(
    c: &Connection,
    id: &str,
    issue_nfse: Option<bool>,
    issue_slip: Option<bool>,
    due_date: Option<&str>,
) -> Result<()> {
    c.execute(
        "UPDATE Billing SET issueNfse = coalesce(?2, issueNfse), \
         issueSlip = coalesce(?3, issueSlip), dueDate = coalesce(?4, dueDate), updatedAt = ?5 \
         WHERE id = ?1",
        params![id, issue_nfse, issue_slip, due_date, now_iso()],
    )?;
    Ok(())
}

pub fn delete(c: &Connection, id: &str) -> Result<()> {
    c.execute("DELETE FROM Billing WHERE id = ?1", [id])?;
    Ok(())
}

/// Allocates a unique, sequential nDPS per billing (never reused, even on
/// retry).
pub fn dps_number(c: &Connection, billing_id: &str) -> Result<i64> {
    c.execute(
        "INSERT OR IGNORE INTO DpsSequence (billingId) VALUES (?1)",
        [billing_id],
    )?;
    Ok(c.query_row(
        "SELECT id FROM DpsSequence WHERE billingId = ?1",
        [billing_id],
        |r| r.get(0),
    )?)
}

#[derive(Debug, Clone, Default)]
pub struct NewInvoice<'a> {
    pub number: Option<&'a str>,
    pub access_key: Option<&'a str>,
    pub xml: Option<&'a str>,
    pub pdf: Option<&'a [u8]>,
    pub raw: Option<&'a str>,
    pub source: &'a str,
}

pub fn insert_invoice(c: &Connection, billing_id: &str, i: &NewInvoice) -> Result<()> {
    c.execute(
        "INSERT INTO Invoice (id, billingId, number, accessKey, xml, pdf, raw, source) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            new_id(),
            billing_id,
            i.number,
            i.access_key,
            i.xml,
            i.pdf,
            i.raw,
            i.source
        ],
    )?;
    Ok(())
}

/// Stores the PDF of an invoice; the number only when one is given.
pub fn set_invoice_pdf(
    c: &Connection,
    billing_id: &str,
    pdf: &[u8],
    number: Option<&str>,
) -> Result<()> {
    c.execute(
        "UPDATE Invoice SET pdf = ?2, number = coalesce(?3, number) WHERE billingId = ?1",
        params![billing_id, pdf, number],
    )?;
    Ok(())
}

pub fn delete_invoice(c: &Connection, billing_id: &str) -> Result<()> {
    c.execute("DELETE FROM Invoice WHERE billingId = ?1", [billing_id])?;
    Ok(())
}

pub fn invoice_exists_with_key(c: &Connection, access_key: &str) -> Result<bool> {
    Ok(c.query_row(
        "SELECT count(*) FROM Invoice WHERE accessKey = ?1",
        [access_key],
        |r| r.get::<_, i64>(0),
    )? > 0)
}

pub struct InvoiceDoc {
    pub source: String,
    pub xml: Option<String>,
    pub pdf: Option<Vec<u8>>,
}

pub fn invoice_doc(c: &Connection, billing_id: &str) -> Result<Option<InvoiceDoc>> {
    Ok(c.query_row(
        "SELECT source, xml, pdf FROM Invoice WHERE billingId = ?1",
        [billing_id],
        |r| {
            Ok(InvoiceDoc {
                source: r.get(0)?,
                xml: r.get(1)?,
                pdf: r.get(2)?,
            })
        },
    )
    .optional()?)
}

#[derive(Debug, Clone, Default)]
pub struct NewSlip<'a> {
    pub codigo_solicitacao: &'a str,
    pub nosso_numero: Option<&'a str>,
    pub linha_digitavel: Option<&'a str>,
    pub barcode: Option<&'a str>,
    pub pdf: Option<&'a [u8]>,
    pub status: Option<&'a str>,
}

pub fn insert_slip(c: &Connection, billing_id: &str, s: &NewSlip) -> Result<()> {
    c.execute(
        "INSERT INTO BankSlip (id, billingId, codigoSolicitacao, nossoNumero, linhaDigitavel, \
         barcode, pdf, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            new_id(),
            billing_id,
            s.codigo_solicitacao,
            s.nosso_numero,
            s.linha_digitavel,
            s.barcode,
            s.pdf,
            s.status
        ],
    )?;
    Ok(())
}

pub fn delete_slip(c: &Connection, billing_id: &str) -> Result<()> {
    c.execute("DELETE FROM BankSlip WHERE billingId = ?1", [billing_id])?;
    Ok(())
}

/// None when there is no boleto; Some(None) when it has no PDF.
pub fn slip_pdf(c: &Connection, billing_id: &str) -> Result<Option<Option<Vec<u8>>>> {
    Ok(c.query_row(
        "SELECT pdf FROM BankSlip WHERE billingId = ?1",
        [billing_id],
        |r| r.get(0),
    )
    .optional()?)
}

pub fn known_slip_codes(c: &Connection) -> Result<std::collections::HashSet<String>> {
    let mut stmt = c.prepare("SELECT codigoSolicitacao FROM BankSlip")?;
    let rows = stmt.query_map([], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub struct DeliveryRecord<'a> {
    pub integration: &'a str,
    pub parts: &'a str,
    pub status: &'a str, // SENT | FAILED
    pub error: Option<&'a str>,
    pub response: Option<&'a str>,
}

/// The outcome of the last attempt; one row per billing.
pub fn record_delivery(c: &Connection, billing_id: &str, d: &DeliveryRecord) -> Result<()> {
    let now = now_iso();
    let sent_at = (d.status == "SENT").then_some(now.as_str());
    c.execute(
        "INSERT INTO Delivery (id, billingId, integration, parts, status, error, response, sentAt, \
         updatedAt) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
         ON CONFLICT(billingId) DO UPDATE SET integration = excluded.integration, \
         parts = excluded.parts, status = excluded.status, error = excluded.error, \
         response = excluded.response, sentAt = excluded.sentAt, updatedAt = excluded.updatedAt",
        params![
            new_id(),
            billing_id,
            d.integration,
            d.parts,
            d.status,
            d.error,
            d.response,
            sent_at,
            now
        ],
    )?;
    Ok(())
}

pub fn delete_delivery(c: &Connection, billing_id: &str) -> Result<()> {
    c.execute("DELETE FROM Delivery WHERE billingId = ?1", [billing_id])?;
    Ok(())
}
