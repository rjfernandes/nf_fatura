use super::billing::BillingService;
use crate::db::billings::{self, DeliveryRecord};
use crate::db::customers::INTEGRATIONS;
use crate::db::Db;
use crate::error::{http_error, Result};
use crate::integrations::types::{
    delivery_field_label, DeliveryParts, DeliveryProvider, DeliveryRequest,
};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

pub type DeliveryProviders = HashMap<&'static str, Arc<dyn DeliveryProvider>>;

/// An integration available for the customer registration.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Integration {
    pub key: String,
    pub label: String,
    pub configured: bool,
    pub required_fields: Vec<&'static str>,
}

pub struct DeliveryService {
    db: Arc<Db>,
    providers: DeliveryProviders,
    /// Gives the PDF of the billing's NFS-e (stored or rendered from the XML).
    billing: Arc<BillingService>,
}

impl DeliveryService {
    pub fn new(db: Arc<Db>, providers: DeliveryProviders, billing: Arc<BillingService>) -> Self {
        Self {
            db,
            providers,
            billing,
        }
    }

    /// Integrations available for the customer registration.
    pub fn list(&self) -> Vec<Integration> {
        INTEGRATIONS
            .iter()
            .filter_map(|key| {
                let p = self.providers.get(key)?;
                Some(Integration {
                    key: key.to_string(),
                    label: p.label().to_string(),
                    configured: p.configured(),
                    required_fields: p.required_fields(),
                })
            })
            .collect()
    }

    /// Sends the NFS-e, the boleto or both through the customer's integration.
    /// The outcome is recorded either way; a failure can be retried.
    pub async fn send(&self, billing_id: &str, parts: DeliveryParts) -> Result<()> {
        let b = self
            .db
            .with(|c| billings::ctx(c, billing_id))?
            .ok_or_else(|| http_error(404, "Faturamento não encontrado"))?;
        let provider = b
            .customer
            .integration
            .as_deref()
            .and_then(|key| self.providers.get_key_value(key));
        let Some((key, provider)) = provider else {
            return Err(http_error(409, "Cliente sem integração de envio"));
        };
        let label = provider.label().to_string();
        if !provider.configured() {
            return Err(http_error(
                409,
                format!("Integração {label} não configurada"),
            ));
        }
        let party = b.customer.party();
        let missing: Vec<&str> = provider
            .required_fields()
            .into_iter()
            .filter(|f| match *f {
                "station" => party.station.as_deref().unwrap_or("").is_empty(),
                _ => false,
            })
            .map(delivery_field_label)
            .collect();
        if !missing.is_empty() {
            return Err(http_error(
                409,
                format!(
                    "Integração {label} incompleta: cliente sem {}",
                    missing.join(", ")
                ),
            ));
        }
        if b.delivery_status.as_deref() == Some("SENT") {
            return Err(http_error(409, "Já enviado ao cliente"));
        }
        let nfse_pdf = if parts == DeliveryParts::Slip || b.invoice.is_none() {
            None
        } else {
            self.billing.nfse_pdf(billing_id)?
        };
        if parts != DeliveryParts::Slip && nfse_pdf.is_none() {
            return Err(http_error(409, "Falta o PDF da NFS-e"));
        }
        let slip_pdf = if parts == DeliveryParts::Nfse {
            None
        } else {
            self.billing.slip_pdf(billing_id)?
        };
        if parts != DeliveryParts::Nfse && slip_pdf.is_none() {
            return Err(http_error(409, "Falta o PDF do boleto"));
        }

        let record = |status: &str, error: Option<&str>, response: Option<&str>| {
            self.db.with(|c| {
                billings::record_delivery(
                    c,
                    billing_id,
                    &DeliveryRecord {
                        integration: key,
                        parts: parts.as_str(),
                        status,
                        error,
                        response,
                    },
                )
            })
        };
        let attempt = async {
            // The remote side is queried first, so an unreachable or refusing
            // integration fails before anything is sent. What it already holds
            // does not stop the send.
            provider
                .already_sent(&party, &b.billing.competence, parts)
                .await?;
            provider
                .send(&DeliveryRequest {
                    customer: party.clone(),
                    competence: b.billing.competence.clone(),
                    nfse_pdf,
                    slip_pdf,
                })
                .await
        };
        match attempt.await {
            Ok(raw) => record("SENT", None, Some(&raw)),
            Err(e) => {
                record("FAILED", Some(&e.message), None)?;
                Err(http_error(502, e.message))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;
    use crate::integrations::types::{NfseResult, SlipResult};
    use crate::services::billing::BillingInput;
    use crate::testing::{customer, FakeDelivery, FakeNfse, FakeSlip, Sent};
    use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
    use std::sync::Mutex;

    const PDF: &[u8] = b"%PDF-1.4 x";

    struct Setup {
        db: Arc<Db>,
        billing: Arc<BillingService>,
        id: String,
    }

    /// Billing with NFS-e and boleto, both with PDF.
    async fn setup(integration: Option<&str>, station: Option<&str>) -> Setup {
        let db = Arc::new(Db::in_memory().unwrap());
        let c = customer(&db, "D", "11222333000181", true);
        db.with(|conn| {
            conn.execute(
                "UPDATE Customer SET integration = ?1, station = ?2",
                rusqlite::params![integration, station],
            )?;
            Ok(())
        })
        .unwrap();
        let billing = Arc::new(BillingService::new(
            db.clone(),
            FakeNfse::issuing(|_| {
                Ok(NfseResult {
                    number: Some("1".into()),
                    pdf: Some(PDF.to_vec()),
                    ..NfseResult::default()
                })
            }),
            FakeSlip::creating(|_| {
                Ok(SlipResult {
                    codigo_solicitacao: "d".into(),
                    pdf: Some(PDF.to_vec()),
                    ..SlipResult::default()
                })
            }),
        ));
        let res = billing
            .run(&BillingInput {
                customer_ids: vec![c.id],
                competence: "2026-09".into(),
                due_date: "2026-10-10".into(),
                mode: None,
            })
            .await
            .unwrap();
        Setup {
            db,
            billing,
            id: res[0].id.clone(),
        }
    }

    fn service(s: &Setup, provider: FakeDelivery) -> (DeliveryService, Arc<Mutex<Vec<Sent>>>) {
        let sent = provider.sent.clone();
        let mut providers: DeliveryProviders = HashMap::new();
        providers.insert("CAMIM", Arc::new(provider));
        (
            DeliveryService::new(s.db.clone(), providers, s.billing.clone()),
            sent,
        )
    }

    fn delivery(s: &Setup) -> Option<billings::BillingDelivery> {
        s.db.with(|c| billings::view(c, &s.id)).unwrap().delivery
    }

    fn drop_slip_pdf(s: &Setup) {
        s.db.with(|c| Ok(c.execute("UPDATE BankSlip SET pdf = NULL", [])?))
            .unwrap();
    }

    #[tokio::test]
    async fn sends_the_pair_once_and_blocks_changes_to_it_afterwards() {
        let s = setup(Some("CAMIM"), Some("A")).await;
        let (svc, sent) = service(&s, FakeDelivery::default());
        svc.send(&s.id, DeliveryParts::Both).await.unwrap();
        assert_eq!(
            *sent.lock().unwrap(),
            [Sent {
                competence: "2026-09".into(),
                nfse: true,
                slip: true
            }]
        );
        let d = delivery(&s).unwrap();
        assert_eq!(d.status, "SENT");
        assert!(d.sent_at.is_some());
        let err = svc.send(&s.id, DeliveryParts::Both).await.unwrap_err();
        assert!(err.message.contains("Já enviado"));
        let err = s.billing.cancel_slip(&s.id).await.unwrap_err();
        assert!(err.message.contains("já enviado"));
        let err = s.billing.attach_nfse_pdf(&s.id, PDF, None).unwrap_err();
        assert!(err.message.contains("já enviada"));
    }

    #[tokio::test]
    async fn sends_only_the_chosen_document_and_records_it() {
        let s = setup(Some("CAMIM"), Some("A")).await;
        let (svc, sent) = service(&s, FakeDelivery::default());
        svc.send(&s.id, DeliveryParts::Nfse).await.unwrap();
        assert_eq!(
            *sent.lock().unwrap(),
            [Sent {
                competence: "2026-09".into(),
                nfse: true,
                slip: false
            }]
        );
        assert_eq!(delivery(&s).unwrap().parts, "NFSE");
    }

    #[tokio::test]
    async fn only_needs_the_pdf_of_the_chosen_document() {
        let s = setup(Some("CAMIM"), Some("A")).await;
        drop_slip_pdf(&s);
        let (svc, sent) = service(&s, FakeDelivery::default());
        for parts in [DeliveryParts::Slip, DeliveryParts::Both] {
            let err = svc.send(&s.id, parts).await.unwrap_err();
            assert!(err.message.contains("PDF do boleto"));
        }
        svc.send(&s.id, DeliveryParts::Nfse).await.unwrap();
        assert_eq!(sent.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn refuses_customers_without_integration_and_pairs_missing_a_pdf() {
        let s = setup(None, Some("A")).await;
        let (svc, _) = service(&s, FakeDelivery::default());
        let err = svc.send(&s.id, DeliveryParts::Both).await.unwrap_err();
        assert!(err.message.contains("sem integração"));
        s.db.with(|c| Ok(c.execute("UPDATE Customer SET integration = 'CAMIM'", [])?))
            .unwrap();
        drop_slip_pdf(&s);
        let err = svc.send(&s.id, DeliveryParts::Both).await.unwrap_err();
        assert!(err.message.contains("PDF do boleto"));
        assert!(delivery(&s).is_none());
    }

    #[tokio::test]
    async fn refuses_when_the_customer_lacks_a_field_the_integration_requires() {
        let s = setup(Some("CAMIM"), None).await;
        let (svc, sent) = service(&s, FakeDelivery::default());
        let err = svc.send(&s.id, DeliveryParts::Both).await.unwrap_err();
        assert_eq!(
            err.message,
            "Integração Camim incompleta: cliente sem posto"
        );
        assert!(sent.lock().unwrap().is_empty());
        assert!(delivery(&s).is_none());
    }

    #[tokio::test]
    async fn refuses_when_the_integration_is_not_configured() {
        let s = setup(Some("CAMIM"), Some("A")).await;
        let (svc, _) = service(
            &s,
            FakeDelivery {
                configured: false,
                ..FakeDelivery::default()
            },
        );
        let err = svc.send(&s.id, DeliveryParts::Both).await.unwrap_err();
        assert!(err.message.contains("não configurada"));
    }

    #[tokio::test]
    async fn records_a_failure_and_lets_it_be_retried() {
        let s = setup(Some("CAMIM"), Some("A")).await;
        let fail = Arc::new(AtomicBool::new(true));
        let f = fail.clone();
        let (svc, _) = service(
            &s,
            FakeDelivery {
                fail: Some(Box::new(move || {
                    f.load(SeqCst)
                        .then(|| AppError::internal("Camim: HTTP 500"))
                })),
                ..FakeDelivery::default()
            },
        );
        let err = svc.send(&s.id, DeliveryParts::Both).await.unwrap_err();
        assert_eq!(err.status, 502);
        assert!(err.message.contains("HTTP 500"));
        let failed = delivery(&s).unwrap();
        assert_eq!(failed.status, "FAILED");
        assert!(failed.error.unwrap().contains("HTTP 500"));
        fail.store(false, SeqCst);
        svc.send(&s.id, DeliveryParts::Both).await.unwrap();
        let ok = delivery(&s).unwrap();
        assert_eq!(ok.status, "SENT");
        assert_eq!(ok.error, None);
    }
}
