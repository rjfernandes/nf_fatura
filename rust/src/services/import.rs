use crate::db::billings::{self, NewBilling, NewInvoice, NewSlip};
use crate::db::customers::{self, CustomerInput};
use crate::db::Db;
use crate::error::Result;
use crate::integrations::types::{ImportedNfse, ImportedSlip, NfseProvider, SlipProvider};
use crate::util::cnpj::normalize_cnpj;
use async_trait::async_trait;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

/// City name and state of an IBGE municipality code.
#[async_trait]
pub trait CityLookup: Send + Sync {
    async fn city_of(&self, code: &str) -> (String, String);
}

pub struct IbgeLookup;

#[async_trait]
impl CityLookup for IbgeLookup {
    async fn city_of(&self, code: &str) -> (String, String) {
        crate::util::ibge::city_of(code).await
    }
}

#[derive(Default)]
struct NewCustomer {
    name: String,
    tax_id: String,
    address: Option<String>,
    number: Option<String>,
    complement: Option<String>,
    neighborhood: Option<String>,
    city: Option<String>,
    city_ibge_code: Option<String>,
    state: Option<String>,
    zipcode: Option<String>,
    recurring_value: i64,
    has_bank_slip: bool,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    #[serde(rename = "ref")]
    pub reference: String,
    pub tax_id: String,
    pub name: Option<String>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: u32, // new billings
    pub linked: u32,   // documents attached to a billing that already existed
    pub duplicates: u32,
    pub created_customers: u32,
    pub skipped: Vec<Skipped>,
}

fn days_from_today(n: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::days(n))
        .format("%Y-%m-%d")
        .to_string()
}

fn or_dash(v: Option<String>, fallback: &str) -> String {
    v.filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

/// Customer by CNPJ, registered from the document when unknown.
fn customer_for(
    c: &Connection,
    by_cnpj: &mut HashMap<String, String>,
    new: NewCustomer,
    result: &mut ImportResult,
) -> Result<Option<String>> {
    let tax_id = normalize_cnpj(&new.tax_id);
    if let Some(id) = by_cnpj.get(&tax_id) {
        return Ok(Some(id.clone()));
    }
    // Only legal entities are customers; an individual's CPF is reported.
    if tax_id.len() != 14 {
        return Ok(None);
    }
    let created = customers::create(
        c,
        &CustomerInput {
            name: or_dash(Some(new.name), &tax_id),
            cnpj: tax_id.clone(),
            address: or_dash(new.address, "-"),
            address_number: or_dash(new.number, "S/N"),
            address_complement: new.complement,
            neighborhood: or_dash(new.neighborhood, "-"),
            city: or_dash(new.city, "-"),
            city_ibge_code: new.city_ibge_code,
            state: or_dash(new.state, "-"),
            zipcode: new.zipcode.unwrap_or_default(),
            recurring_value: new.recurring_value,
            has_bank_slip: new.has_bank_slip,
            ..CustomerInput::default()
        },
    )?;
    by_cnpj.insert(tax_id, created.id.clone());
    result.created_customers += 1;
    Ok(Some(created.id))
}

fn customer_ids(c: &Connection) -> Result<HashMap<String, String>> {
    Ok(customers::list(c)?
        .into_iter()
        .map(|c| (normalize_cnpj(&c.cnpj), c.id))
        .collect())
}

/// The billing is done once every document it asks for is there.
fn settle(c: &Connection, billing_id: &str) -> Result<()> {
    if let Some(b) = billings::ctx(c, billing_id)? {
        let complete = (!b.billing.issue_nfse || b.invoice.is_some())
            && (!b.billing.issue_slip || b.slip.is_some());
        if complete {
            billings::set_status(c, billing_id, "COMPLETED", None)?;
        }
    }
    Ok(())
}

fn attach_invoice(
    c: &Connection,
    customer_id: &str,
    n: &ImportedNfse,
    result: &mut ImportResult,
) -> Result<()> {
    let date: String = n.issued_at.chars().take(10).collect();
    let invoice = NewInvoice {
        number: Some(&n.number),
        access_key: Some(&n.access_key),
        xml: Some(&n.xml),
        source: "IMPORTED",
        ..NewInvoice::default()
    };
    if let Some(existing) =
        billings::find_for_competence(c, customer_id, &n.competence, Some("Invoice"))?
    {
        billings::set_wanted(c, &existing.id, Some(true), None, None)?;
        billings::insert_invoice(c, &existing.id, &invoice)?;
        settle(c, &existing.id)?;
        result.linked += 1;
        return Ok(());
    }
    let id = billings::create(
        c,
        &NewBilling {
            customer_id,
            competence: &n.competence,
            amount_cents: n.amount_cents,
            due_date: &date,
            issue_nfse: true,
            issue_slip: false,
            status: "COMPLETED",
        },
    )?;
    billings::insert_invoice(c, &id, &invoice)?;
    result.imported += 1;
    Ok(())
}

fn attach_slip(
    c: &Connection,
    customer_id: &str,
    s: &ImportedSlip,
    result: &mut ImportResult,
) -> Result<()> {
    // The due date is the competence.
    let competence: String = s.due_date.chars().take(7).collect();
    let slip = NewSlip {
        codigo_solicitacao: &s.codigo_solicitacao,
        nosso_numero: s.nosso_numero.as_deref(),
        linha_digitavel: s.linha_digitavel.as_deref(),
        barcode: s.barcode.as_deref(),
        status: s.status.as_deref(),
        pdf: s.pdf.as_deref(),
    };
    if let Some(existing) =
        billings::find_for_competence(c, customer_id, &competence, Some("BankSlip"))?
    {
        billings::set_wanted(c, &existing.id, None, Some(true), Some(&s.due_date))?;
        billings::insert_slip(c, &existing.id, &slip)?;
        settle(c, &existing.id)?;
        result.linked += 1;
        return Ok(());
    }
    let id = billings::create(
        c,
        &NewBilling {
            customer_id,
            competence: &competence,
            amount_cents: s.amount_cents,
            due_date: &s.due_date,
            issue_nfse: false,
            issue_slip: true,
            status: "COMPLETED",
        },
    )?;
    billings::insert_slip(c, &id, &slip)?;
    result.imported += 1;
    Ok(())
}

/// Brings documents issued outside the system into the history: the NFS-e from
/// the national portal and the boletos from the bank. They can come in any
/// order; each one joins the billing of its customer + competence when there is
/// one (without that document yet), otherwise a billing is created.
pub struct ImportService {
    db: Arc<Db>,
    nfse: Arc<dyn NfseProvider>,
    slip: Arc<dyn SlipProvider>,
    cities: Arc<dyn CityLookup>,
}

impl ImportService {
    pub fn new(
        db: Arc<Db>,
        nfse: Arc<dyn NfseProvider>,
        slip: Arc<dyn SlipProvider>,
        cities: Arc<dyn CityLookup>,
    ) -> Self {
        Self {
            db,
            nfse,
            slip,
            cities,
        }
    }

    pub async fn import_from_nfse(&self) -> Result<ImportResult> {
        let notes = self.nfse.list().await?;
        let mut by_cnpj = self.db.with(|c| customer_ids(c))?;
        let mut result = ImportResult::default();
        for n in notes {
            if self
                .db
                .with(|c| billings::invoice_exists_with_key(c, &n.access_key))?
            {
                result.duplicates += 1;
                continue;
            }
            let a = &n.taker_address;
            let known = by_cnpj.contains_key(&normalize_cnpj(&n.taker_tax_id));
            let (city, state) = if known {
                (None, None)
            } else {
                let (city, state) = self
                    .cities
                    .city_of(a.city_ibge_code.as_deref().unwrap_or(""))
                    .await;
                (Some(city), Some(state))
            };
            let new = NewCustomer {
                name: n.taker_name.clone().unwrap_or_default(),
                tax_id: n.taker_tax_id.clone(),
                address: a.address.clone(),
                number: a.number.clone(),
                complement: a.complement.clone(),
                neighborhood: a.neighborhood.clone(),
                city,
                city_ibge_code: a.city_ibge_code.clone(),
                state,
                zipcode: a.zipcode.clone(),
                recurring_value: n.amount_cents,
                has_bank_slip: false,
            };
            self.db
                .with(|c| match customer_for(c, &mut by_cnpj, new, &mut result)? {
                    Some(customer_id) => attach_invoice(c, &customer_id, &n, &mut result),
                    None => {
                        result.skipped.push(Skipped {
                            reference: format!("NFS-e {}", n.number),
                            tax_id: n.taker_tax_id.clone(),
                            name: n.taker_name.clone(),
                        });
                        Ok(())
                    }
                })?;
        }
        Ok(result)
    }

    /// Boletos due from ~10 months back to 2 months ahead (the bank caps a query
    /// at a year).
    pub async fn import_from_slips(&self) -> Result<ImportResult> {
        let (from, to) = (days_from_today(-300), days_from_today(60));
        let known = self.db.with(|c| billings::known_slip_codes(c))?;
        let slips = self.slip.list(&from, &to, &known).await?;
        let mut result = ImportResult::default();
        self.db.with(|c| {
            let mut by_cnpj = customer_ids(c)?;
            for s in &slips {
                if known.contains(&s.codigo_solicitacao) {
                    result.duplicates += 1;
                    continue;
                }
                let p = &s.payer;
                let new = NewCustomer {
                    name: p.name.clone(),
                    tax_id: p.tax_id.clone(),
                    address: p.address.clone(),
                    number: p.number.clone(),
                    complement: p.complement.clone(),
                    neighborhood: p.neighborhood.clone(),
                    city: p.city.clone(),
                    city_ibge_code: None,
                    state: p.state.clone(),
                    zipcode: p.zipcode.clone(),
                    recurring_value: s.amount_cents,
                    has_bank_slip: true,
                };
                match customer_for(c, &mut by_cnpj, new, &mut result)? {
                    Some(customer_id) => attach_slip(c, &customer_id, s, &mut result)?,
                    None => result.skipped.push(Skipped {
                        reference: format!(
                            "Boleto {}",
                            s.nosso_numero.as_deref().unwrap_or(&s.codigo_solicitacao)
                        ),
                        tax_id: p.tax_id.clone(),
                        name: Some(p.name.clone()),
                    }),
                }
            }
            Ok(())
        })?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::types::{NfseResult, SlipPayer, SlipResult};
    use crate::testing::{FakeNfse, FakeSlip, NoCities};

    fn note(key: &str, tax_id: &str) -> ImportedNfse {
        ImportedNfse {
            access_key: key.into(),
            number: "10".into(),
            xml: "<x/>".into(),
            issued_at: "2026-09-30T23:50:00-03:00".into(),
            competence: "2026-09".into(),
            taker_tax_id: tax_id.into(),
            taker_name: Some("Acme".into()),
            amount_cents: 5000,
            ..ImportedNfse::default()
        }
    }

    fn slip() -> ImportedSlip {
        ImportedSlip {
            codigo_solicitacao: "S1".into(),
            due_date: "2026-09-10".into(),
            amount_cents: 5000,
            payer: SlipPayer {
                tax_id: "11.222.333/0001-81".into(),
                name: "Acme".into(),
                ..SlipPayer::default()
            },
            ..ImportedSlip::default()
        }
    }

    fn service(db: &Arc<Db>, notes: Vec<ImportedNfse>, slips: Vec<ImportedSlip>) -> ImportService {
        ImportService::new(
            db.clone(),
            Arc::new(FakeNfse {
                issue: Box::new(|_| Ok(NfseResult::default())),
                notes,
            }),
            Arc::new(FakeSlip {
                create: Box::new(|_| Ok(SlipResult::default())),
                cancel: Box::new(|_| Ok(())),
                slips,
            }),
            Arc::new(NoCities),
        )
    }

    #[tokio::test]
    async fn joins_the_nfse_and_the_boleto_of_the_same_competence_in_any_order() {
        for slip_first in [false, true] {
            let db = Arc::new(Db::in_memory().unwrap());
            let svc = service(&db, vec![note("K1", "11222333000181")], vec![slip()]);
            let (first, second) = if slip_first {
                (
                    svc.import_from_slips().await.unwrap(),
                    svc.import_from_nfse().await.unwrap(),
                )
            } else {
                (
                    svc.import_from_nfse().await.unwrap(),
                    svc.import_from_slips().await.unwrap(),
                )
            };
            assert_eq!((first.imported, first.created_customers), (1, 1));
            assert_eq!((second.linked, second.created_customers), (1, 0));
            let all = db.with(|c| billings::list(c)).unwrap();
            assert_eq!(all.len(), 1);
            let b = &all[0];
            assert_eq!(b.competence, "2026-09");
            assert_eq!(b.status, "COMPLETED");
            assert!(b.issue_nfse && b.issue_slip);
            assert_eq!(b.invoice.as_ref().unwrap().source, "IMPORTED");
            assert!(b.bank_slip.is_some());
            let customer = &db.with(|c| customers::list(c)).unwrap()[0];
            assert_eq!(customer.cnpj, "11222333000181");
            assert_eq!(customer.recurring_value, 5000);
            assert_eq!(customer.has_bank_slip, slip_first);
        }
    }

    #[tokio::test]
    async fn does_not_import_the_same_documents_twice() {
        let db = Arc::new(Db::in_memory().unwrap());
        let svc = service(&db, vec![note("K1", "11222333000181")], vec![slip()]);
        svc.import_from_nfse().await.unwrap();
        svc.import_from_slips().await.unwrap();
        let again = svc.import_from_nfse().await.unwrap();
        assert_eq!((again.duplicates, again.imported), (1, 0));
        let again = svc.import_from_slips().await.unwrap();
        assert_eq!((again.duplicates, again.imported), (1, 0));
    }

    #[tokio::test]
    async fn reports_documents_of_individuals_instead_of_registering_them() {
        let db = Arc::new(Db::in_memory().unwrap());
        let svc = service(&db, vec![note("K2", "12345678901")], vec![]);
        let r = svc.import_from_nfse().await.unwrap();
        assert_eq!((r.imported, r.created_customers), (0, 0));
        assert_eq!(
            r.skipped,
            [Skipped {
                reference: "NFS-e 10".into(),
                tax_id: "12345678901".into(),
                name: Some("Acme".into()),
            }]
        );
    }
}
