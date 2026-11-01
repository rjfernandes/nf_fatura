use crate::db::billings::{self, BillingView, NewBilling, NewInvoice, NewSlip};
use crate::db::{customers, Db};
use crate::error::{http_error, AppError, Result};
use crate::integrations::nfse::danfse::{render_danfse, DanfseExtras};
use crate::integrations::types::{NfseProvider, NfseRequest, SlipProvider, SlipRequest};
use crate::util::period::{is_competence, is_date};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const MAX_PDF_BYTES: usize = 10 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillingInput {
    pub customer_ids: Vec<String>,
    pub competence: String,
    pub due_date: String,
    /// AUTO follows each customer's registration: NFS-e always, boleto only if
    /// hasBankSlip.
    #[serde(default)]
    pub mode: Option<String>,
}

impl BillingInput {
    pub fn validate(&self) -> Result<()> {
        let mut issues = Vec::new();
        if self.customer_ids.is_empty() {
            issues.push("customerIds selecione ao menos um cliente");
        }
        if !is_competence(&self.competence) {
            issues.push("competence deve ser AAAA-MM");
        }
        if !is_date(&self.due_date) {
            issues.push("dueDate deve ser AAAA-MM-DD");
        }
        if !matches!(
            self.mode.as_deref(),
            None | Some("AUTO" | "NFSE" | "SLIP" | "BOTH")
        ) {
            issues.push("mode inválido");
        }
        if issues.is_empty() {
            Ok(())
        } else {
            Err(AppError::new(
                400,
                format!("Dados inválidos: {}", issues.join("; ")),
            ))
        }
    }
}

/// (issue NFS-e, issue boleto)
fn resolve_mode(mode: &str, has_bank_slip: bool) -> (bool, bool) {
    match mode {
        "NFSE" => (true, false),
        "SLIP" => (false, true),
        "BOTH" => (true, true),
        _ => (true, has_bank_slip),
    }
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SlipCancelled {
    pub billing_removed: bool,
}

pub struct BillingService {
    db: Arc<Db>,
    nfse: Arc<dyn NfseProvider>,
    slip: Arc<dyn SlipProvider>,
}

impl BillingService {
    pub fn new(db: Arc<Db>, nfse: Arc<dyn NfseProvider>, slip: Arc<dyn SlipProvider>) -> Self {
        Self { db, nfse, slip }
    }

    /// Creates one Billing per customer and processes each independently.
    pub async fn run(&self, input: &BillingInput) -> Result<Vec<BillingView>> {
        let mode = input.mode.as_deref().unwrap_or("AUTO");
        let ids = self.db.with(|c| {
            let mut ids = Vec::new();
            for customer in customers::list(c)? {
                if !input.customer_ids.contains(&customer.id) {
                    continue;
                }
                let (issue_nfse, issue_slip) = resolve_mode(mode, customer.has_bank_slip);
                ids.push(billings::create(
                    c,
                    &NewBilling {
                        customer_id: &customer.id,
                        competence: &input.competence,
                        amount_cents: customer.recurring_value,
                        due_date: &input.due_date,
                        issue_nfse,
                        issue_slip,
                        status: "PENDING",
                    },
                )?);
            }
            Ok(ids)
        })?;
        // Sequential to respect API rate limits and keep ordering predictable.
        for id in &ids {
            self.process(id).await?;
        }
        self.db
            .with(|c| ids.iter().map(|id| billings::view(c, id)).collect())
    }

    /// Cancels the boleto at the bank and then drops it locally. The bank goes
    /// first: if it refuses (already paid, for instance) nothing changes here.
    /// Without an NFS-e the billing has nothing left, so it leaves the history
    /// too; with one, the billing stays (an issued NFS-e must not vanish) and
    /// issueSlip is cleared so a retry won't recreate the boleto.
    pub async fn cancel_slip(&self, billing_id: &str) -> Result<SlipCancelled> {
        let b = self
            .db
            .with(|c| billings::ctx(c, billing_id))?
            .ok_or_else(|| http_error(404, "Faturamento não encontrado"))?;
        let Some(slip) = &b.slip else {
            return Err(http_error(404, "Boleto não encontrado"));
        };
        if b.delivery_status.as_deref() == Some("SENT") {
            return Err(http_error(409, "Boleto já enviado ao cliente"));
        }
        self.slip
            .cancel(&slip.codigo_solicitacao, "Cancelado pelo emissor")
            .await?;
        let keep = b.invoice.is_some();
        self.db.with(|c| {
            let tx = c.transaction()?;
            billings::delete_slip(&tx, billing_id)?;
            if keep {
                billings::set_wanted(&tx, billing_id, None, Some(false), None)?;
            } else {
                billings::delete_delivery(&tx, billing_id)?;
                billings::delete(&tx, billing_id)?;
            }
            tx.commit()?;
            Ok(())
        })?;
        Ok(SlipCancelled {
            billing_removed: !keep,
        })
    }

    /// Attaches the PDF of an NFS-e issued outside the system (Emissor Web). It
    /// becomes a MANUAL invoice, so process() won't issue another one through the
    /// API. An API invoice only gets the PDF when it has none (DANFSe download
    /// failed); an official PDF is never overwritten.
    pub fn attach_nfse_pdf(
        &self,
        billing_id: &str,
        pdf: &[u8],
        number: Option<&str>,
    ) -> Result<()> {
        if !pdf.starts_with(b"%PDF-") {
            return Err(http_error(400, "Envie um arquivo PDF"));
        }
        if pdf.len() > MAX_PDF_BYTES {
            return Err(http_error(413, "O PDF deve ter no máximo 10 MB."));
        }
        let number = number.map(str::trim).filter(|n| !n.is_empty());
        if number.is_some_and(|n| n.chars().count() > 20) {
            return Err(http_error(400, "Dados inválidos: number até 20 caracteres"));
        }
        self.db.with(|c| {
            let b = billings::ctx(c, billing_id)?
                .ok_or_else(|| http_error(404, "Faturamento não encontrado"))?;
            if b.delivery_status.as_deref() == Some("SENT") {
                return Err(http_error(409, "NFS-e já enviada ao cliente"));
            }
            let Some(invoice) = &b.invoice else {
                let tx = c.transaction()?;
                billings::insert_invoice(
                    &tx,
                    billing_id,
                    &NewInvoice {
                        source: "MANUAL",
                        pdf: Some(pdf),
                        number,
                        ..NewInvoice::default()
                    },
                )?;
                billings::set_wanted(&tx, billing_id, Some(true), None, None)?;
                tx.commit()?;
                return Ok(());
            };
            if invoice.source == "MANUAL" {
                return billings::set_invoice_pdf(c, billing_id, pdf, number);
            }
            if invoice.has_stored_pdf {
                return Err(http_error(409, "Esta NFS-e já tem o PDF oficial"));
            }
            billings::set_invoice_pdf(c, billing_id, pdf, None)
        })
    }

    /// Removes a MANUAL invoice. With a boleto, issueNfse is cleared so a retry
    /// won't issue a new NFS-e through the API.
    pub fn remove_manual_nfse(&self, billing_id: &str) -> Result<()> {
        self.db.with(|c| {
            let b = billings::ctx(c, billing_id)?;
            let Some((b, invoice)) = b.and_then(|b| b.invoice.clone().map(|i| (b, i))) else {
                return Err(http_error(404, "NFS-e não encontrada"));
            };
            if b.delivery_status.as_deref() == Some("SENT") {
                return Err(http_error(409, "NFS-e já enviada ao cliente"));
            }
            if invoice.source != "MANUAL" {
                return Err(http_error(
                    409,
                    "Só é possível remover NFS-e anexada manualmente",
                ));
            }
            let tx = c.transaction()?;
            billings::delete_invoice(&tx, billing_id)?;
            if b.slip.is_some() {
                billings::set_wanted(&tx, billing_id, Some(false), None, None)?;
            }
            tx.commit()?;
            Ok(())
        })
    }

    /// Only failed billings without an issued NFS-e or boleto can be discarded.
    pub fn remove(&self, billing_id: &str) -> Result<()> {
        self.db.with(|c| {
            let b =
                billings::ctx(c, billing_id)?.ok_or_else(|| http_error(404, "Não encontrado"))?;
            if b.billing.status != "FAILED" {
                return Err(http_error(
                    409,
                    "Só é possível remover faturamentos com erro",
                ));
            }
            if b.invoice.is_some() || b.slip.is_some() {
                return Err(http_error(
                    409,
                    "Este faturamento já tem NFS-e ou boleto emitido; não pode ser removido",
                ));
            }
            billings::delete(c, billing_id)
        })
    }

    /// PDF of the NFS-e: rendered from the XML when issued here, else stored.
    pub fn nfse_pdf(&self, billing_id: &str) -> Result<Option<Vec<u8>>> {
        let (doc, customer) = self.db.with(|c| {
            Ok((
                billings::invoice_doc(c, billing_id)?,
                billings::ctx(c, billing_id)?.map(|b| b.customer),
            ))
        })?;
        let Some(doc) = doc else {
            return Ok(None);
        };
        match (&doc.xml, doc.source.as_str()) {
            (Some(xml), source) if source != "MANUAL" => {
                let extras = DanfseExtras {
                    customer_city: customer.as_ref().map(|c| c.city.clone()),
                    customer_state: customer.as_ref().map(|c| c.state.clone()),
                };
                Ok(Some(render_danfse(xml, &extras)?))
            }
            _ => Ok(doc.pdf),
        }
    }

    pub fn nfse_xml(&self, billing_id: &str) -> Result<Option<String>> {
        self.db
            .with(|c| Ok(billings::invoice_doc(c, billing_id)?.and_then(|d| d.xml)))
    }

    pub fn slip_pdf(&self, billing_id: &str) -> Result<Option<Vec<u8>>> {
        self.db
            .with(|c| Ok(billings::slip_pdf(c, billing_id)?.flatten()))
    }

    /// Resumes from the failed step; safe to call on FAILED billings.
    pub async fn process(&self, billing_id: &str) -> Result<()> {
        let b = self
            .db
            .with(|c| billings::ctx(c, billing_id))?
            .ok_or_else(|| http_error(404, "Faturamento não encontrado"))?;
        let steps = async {
            let row = &b.billing;
            if row.issue_nfse && b.invoice.is_none() {
                let dps_number = self.db.with(|c| billings::dps_number(c, &row.id))?;
                let r = self
                    .nfse
                    .issue(&NfseRequest {
                        billing_id: row.id.clone(),
                        dps_number,
                        customer: b.customer.party(),
                        amount_cents: row.amount_cents,
                        competence: row.competence.clone(),
                    })
                    .await?;
                self.db.with(|c| {
                    billings::insert_invoice(
                        c,
                        &row.id,
                        &NewInvoice {
                            number: r.number.as_deref(),
                            access_key: r.access_key.as_deref(),
                            xml: r.xml.as_deref(),
                            pdf: r.pdf.as_deref(),
                            raw: r.raw.as_deref(),
                            source: "API",
                        },
                    )?;
                    billings::set_status(c, &row.id, "NFSE_ISSUED", None)
                })?;
            }
            if row.issue_slip && b.slip.is_none() {
                let s = self
                    .slip
                    .create(&SlipRequest {
                        billing_id: row.id.clone(),
                        customer: b.customer.party(),
                        amount_cents: row.amount_cents,
                        due_date: row.due_date.clone(),
                    })
                    .await?;
                self.db.with(|c| {
                    billings::insert_slip(
                        c,
                        &row.id,
                        &NewSlip {
                            codigo_solicitacao: &s.codigo_solicitacao,
                            nosso_numero: s.nosso_numero.as_deref(),
                            linha_digitavel: s.linha_digitavel.as_deref(),
                            barcode: s.barcode.as_deref(),
                            pdf: s.pdf.as_deref(),
                            status: s.status.as_deref(),
                        },
                    )
                })?;
            }
            self.db
                .with(|c| billings::set_status(c, &row.id, "COMPLETED", None))
        };
        let outcome: Result<()> = steps.await;
        if let Err(e) = outcome {
            self.db
                .with(|c| billings::set_status(c, billing_id, "FAILED", Some(&e.message)))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::types::{NfseResult, SlipResult};
    use crate::testing::{customer, FakeNfse, FakeSlip};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
    use std::sync::Mutex;

    const PDF: &[u8] = b"%PDF-1.4 manual";

    fn input(ids: &[&str], mode: Option<&str>) -> BillingInput {
        BillingInput {
            customer_ids: ids.iter().map(|s| s.to_string()).collect(),
            competence: "2026-09".into(),
            due_date: "2026-10-10".into(),
            mode: mode.map(str::to_string),
        }
    }

    fn numbered(n: &str) -> NfseResult {
        NfseResult {
            number: Some(n.into()),
            ..NfseResult::default()
        }
    }

    fn slip(code: &str) -> SlipResult {
        SlipResult {
            codigo_solicitacao: code.into(),
            ..SlipResult::default()
        }
    }

    fn count(db: &Db, sql: &str) -> i64 {
        db.with(|c| Ok(c.query_row(sql, [], |r| r.get(0))?))
            .unwrap()
    }

    #[tokio::test]
    async fn isolates_failures_per_customer_and_creates_slips_only_when_enabled() {
        let db = Arc::new(Db::in_memory().unwrap());
        let ok = customer(&db, "OK", "11222333000181", true);
        let no_slip = customer(&db, "NoSlip", "22333444000181", false);
        let bad = customer(&db, "Bad", "33444555000181", true);
        let slips = Arc::new(AtomicUsize::new(0));
        let seen = slips.clone();
        let svc = BillingService::new(
            db.clone(),
            FakeNfse::issuing(|r| {
                if r.customer.name == "Bad" {
                    return Err(AppError::internal("boom"));
                }
                Ok(NfseResult {
                    number: Some("1".into()),
                    xml: Some("<x/>".into()),
                    ..NfseResult::default()
                })
            }),
            FakeSlip::creating(move |_| {
                seen.fetch_add(1, SeqCst);
                Ok(slip("abc"))
            }),
        );
        let res = svc
            .run(&input(&[&ok.id, &no_slip.id, &bad.id], None))
            .await
            .unwrap();
        let by = |name: &str| res.iter().find(|b| b.customer.name == name).unwrap();
        assert_eq!(by("OK").status, "COMPLETED");
        assert_eq!(by("NoSlip").status, "COMPLETED");
        assert_eq!(by("Bad").status, "FAILED");
        assert_eq!(by("Bad").error.as_deref(), Some("boom"));
        assert_eq!(slips.load(SeqCst), 1);
    }

    #[tokio::test]
    async fn each_mode_issues_its_documents() {
        // customer has hasBankSlip=false
        for (mode, nfse, slips) in [
            ("NFSE", 1, 0),
            ("SLIP", 0, 1),
            ("BOTH", 1, 1),
            ("AUTO", 1, 0),
        ] {
            let db = Arc::new(Db::in_memory().unwrap());
            let c = customer(&db, "M", "11222333000181", false);
            let issued = Arc::new(AtomicUsize::new(0));
            let created = Arc::new(AtomicUsize::new(0));
            let (i, s) = (issued.clone(), created.clone());
            let svc = BillingService::new(
                db.clone(),
                FakeNfse::issuing(move |_| {
                    i.fetch_add(1, SeqCst);
                    Ok(numbered("1"))
                }),
                FakeSlip::creating(move |_| {
                    s.fetch_add(1, SeqCst);
                    Ok(slip("a"))
                }),
            );
            let res = svc.run(&input(&[&c.id], Some(mode))).await.unwrap();
            assert_eq!(res[0].status, "COMPLETED", "{mode}");
            assert_eq!(
                (issued.load(SeqCst), created.load(SeqCst)),
                (nfse, slips),
                "{mode}"
            );
        }
    }

    #[tokio::test]
    async fn retry_resumes_without_reissuing_the_nfse() {
        let db = Arc::new(Db::in_memory().unwrap());
        let c = customer(&db, "R", "11222333000181", true);
        let issued = Arc::new(AtomicUsize::new(0));
        let fail = Arc::new(AtomicBool::new(true));
        let (i, f) = (issued.clone(), fail.clone());
        let svc = BillingService::new(
            db.clone(),
            FakeNfse::issuing(move |_| {
                i.fetch_add(1, SeqCst);
                Ok(numbered("9"))
            }),
            FakeSlip::creating(move |_| {
                if f.load(SeqCst) {
                    return Err(AppError::internal("inter down"));
                }
                Ok(slip("z"))
            }),
        );
        let res = svc.run(&input(&[&c.id], None)).await.unwrap();
        assert_eq!(res[0].status, "FAILED");
        assert_eq!(res[0].error.as_deref(), Some("inter down"));
        fail.store(false, SeqCst);
        svc.process(&res[0].id).await.unwrap();
        let after = db.with(|c| billings::view(c, &res[0].id)).unwrap();
        assert_eq!(after.status, "COMPLETED");
        assert_eq!(after.error, None);
        assert_eq!(issued.load(SeqCst), 1);
    }

    #[tokio::test]
    async fn cancel_slip_cancels_at_the_bank_drops_the_local_slip_and_stops_retries() {
        let db = Arc::new(Db::in_memory().unwrap());
        let c = customer(&db, "C", "11222333000181", true);
        let cancelled = Arc::new(Mutex::new(Vec::<String>::new()));
        let refuse = Arc::new(AtomicBool::new(true));
        let (seen, r) = (cancelled.clone(), refuse.clone());
        let svc = BillingService::new(
            db.clone(),
            FakeNfse::issuing(|_| Ok(numbered("1"))),
            Arc::new(FakeSlip {
                create: Box::new(|_| Ok(slip("cod1"))),
                cancel: Box::new(move |code| {
                    if r.load(SeqCst) {
                        return Err(AppError::internal("já pago"));
                    }
                    seen.lock().unwrap().push(code.to_string());
                    Ok(())
                }),
                slips: vec![],
            }),
        );
        let b = &svc.run(&input(&[&c.id], None)).await.unwrap()[0];
        assert!(svc
            .cancel_slip(&b.id)
            .await
            .unwrap_err()
            .message
            .contains("já pago"));
        // bank refused: nothing changed
        assert_eq!(count(&db, "SELECT count(*) FROM BankSlip"), 1);
        refuse.store(false, SeqCst);
        // has an NFS-e: billing stays
        assert_eq!(
            svc.cancel_slip(&b.id).await.unwrap(),
            SlipCancelled {
                billing_removed: false
            }
        );
        assert_eq!(*cancelled.lock().unwrap(), ["cod1"]);
        assert_eq!(count(&db, "SELECT count(*) FROM BankSlip"), 0);
        assert!(!db.with(|c| billings::view(c, &b.id)).unwrap().issue_slip);
    }

    #[tokio::test]
    async fn cancel_slip_also_removes_a_boleto_only_billing_from_the_history() {
        let db = Arc::new(Db::in_memory().unwrap());
        let c = customer(&db, "S", "11222333000181", true);
        let svc = BillingService::new(
            db.clone(),
            FakeNfse::issuing(|_| Ok(numbered("1"))),
            FakeSlip::creating(|_| Ok(slip("cod2"))),
        );
        let b = &svc.run(&input(&[&c.id], Some("SLIP"))).await.unwrap()[0];
        assert_eq!(
            svc.cancel_slip(&b.id).await.unwrap(),
            SlipCancelled {
                billing_removed: true
            }
        );
        assert_eq!(count(&db, "SELECT count(*) FROM Billing"), 0);
    }

    #[tokio::test]
    async fn attaches_to_a_boleto_only_billing_as_a_manual_invoice_and_can_be_removed() {
        let db = Arc::new(Db::in_memory().unwrap());
        let c = customer(&db, "A", "11222333000181", true);
        let svc = BillingService::new(
            db.clone(),
            FakeNfse::issuing(|_| Ok(numbered("1"))),
            FakeSlip::creating(|_| Ok(slip("a"))),
        );
        let b = &svc.run(&input(&[&c.id], Some("SLIP"))).await.unwrap()[0];
        svc.attach_nfse_pdf(&b.id, PDF, Some("123")).unwrap();
        let after = db.with(|c| billings::view(c, &b.id)).unwrap();
        assert_eq!(after.status, "COMPLETED");
        assert!(after.issue_nfse);
        let invoice = after.invoice.unwrap();
        assert_eq!(invoice.source, "MANUAL");
        assert_eq!(invoice.number.as_deref(), Some("123"));
        assert!(invoice.has_pdf);
        assert_eq!(svc.nfse_pdf(&b.id).unwrap().as_deref(), Some(PDF));

        svc.remove_manual_nfse(&b.id).unwrap();
        let removed = db.with(|c| billings::view(c, &b.id)).unwrap();
        assert!(removed.invoice.is_none());
        assert!(!removed.issue_nfse);
    }

    #[tokio::test]
    async fn lets_a_failed_billing_be_retried_without_issuing_the_nfse_through_the_api() {
        let db = Arc::new(Db::in_memory().unwrap());
        let c = customer(&db, "F", "11222333000181", true);
        let issued = Arc::new(AtomicUsize::new(0));
        let fail = Arc::new(AtomicBool::new(true));
        let (i, f) = (issued.clone(), fail.clone());
        let svc = BillingService::new(
            db.clone(),
            FakeNfse::issuing(move |_| {
                i.fetch_add(1, SeqCst);
                if f.load(SeqCst) {
                    return Err(AppError::internal("sefin down"));
                }
                Ok(numbered("1"))
            }),
            FakeSlip::creating(|_| Ok(slip("f"))),
        );
        let b = &svc.run(&input(&[&c.id], None)).await.unwrap()[0];
        assert_eq!(b.status, "FAILED");
        svc.attach_nfse_pdf(&b.id, PDF, None).unwrap();
        fail.store(false, SeqCst);
        svc.process(&b.id).await.unwrap();
        let after = db.with(|c| billings::view(c, &b.id)).unwrap();
        assert_eq!(after.status, "COMPLETED");
        assert!(after.bank_slip.is_some());
        assert_eq!(issued.load(SeqCst), 1);
    }

    #[tokio::test]
    async fn fills_a_missing_pdf_of_an_api_invoice_but_never_replaces_an_existing_one() {
        let db = Arc::new(Db::in_memory().unwrap());
        let c = customer(&db, "P", "11222333000181", false);
        let svc = BillingService::new(
            db.clone(),
            FakeNfse::issuing(|_| {
                Ok(NfseResult {
                    number: Some("7".into()),
                    xml: Some("<x/>".into()),
                    ..NfseResult::default()
                })
            }),
            FakeSlip::creating(|_| Ok(slip("p"))),
        );
        let b = &svc.run(&input(&[&c.id], None)).await.unwrap()[0];
        svc.attach_nfse_pdf(&b.id, PDF, Some("999")).unwrap();
        let invoice = db
            .with(|c| billings::view(c, &b.id))
            .unwrap()
            .invoice
            .unwrap();
        assert_eq!(invoice.source, "API");
        assert_eq!(invoice.number.as_deref(), Some("7"));
        let stored = db
            .with(|c| billings::invoice_doc(c, &b.id))
            .unwrap()
            .unwrap()
            .pdf;
        assert_eq!(stored.as_deref(), Some(PDF));
        assert!(svc
            .attach_nfse_pdf(&b.id, PDF, None)
            .unwrap_err()
            .message
            .contains("já tem o PDF oficial"));
        assert!(svc
            .remove_manual_nfse(&b.id)
            .unwrap_err()
            .message
            .contains("anexada manualmente"));
    }

    #[tokio::test]
    async fn only_discards_failed_billings_without_documents_and_rejects_non_pdfs() {
        let db = Arc::new(Db::in_memory().unwrap());
        let c = customer(&db, "X", "11222333000181", false);
        let fail = Arc::new(AtomicBool::new(true));
        let f = fail.clone();
        let svc = BillingService::new(
            db.clone(),
            FakeNfse::issuing(move |_| {
                if f.load(SeqCst) {
                    return Err(AppError::internal("down"));
                }
                Ok(numbered("1"))
            }),
            FakeSlip::creating(|_| Ok(slip("x"))),
        );
        let failed = &svc.run(&input(&[&c.id], None)).await.unwrap()[0];
        assert_eq!(
            svc.attach_nfse_pdf(&failed.id, b"<html>", None)
                .unwrap_err()
                .status,
            400
        );
        fail.store(false, SeqCst);
        let done = &svc.run(&input(&[&c.id], None)).await.unwrap()[0];
        assert_eq!(svc.remove(&done.id).unwrap_err().status, 409);
        svc.remove(&failed.id).unwrap();
        assert_eq!(svc.remove(&failed.id).unwrap_err().status, 404);
        assert_eq!(input(&[], Some("X")).validate().unwrap_err().status, 400);
    }
}
