use super::types::{AlreadySent, BillingParty, DeliveryParts, DeliveryProvider, DeliveryRequest};
use crate::error::{AppError, Result};
use async_trait::async_trait;
use reqwest::multipart::{Form, Part};
use serde::Deserialize;

/// Camim PJ portal: the boleto of the month and its NFS-e, together or one at a
/// time. A single token identifies us; the customer's posto tells which clinic it is.
pub struct CamimDeliveryProvider {
    base_url: String,
    token: Option<String>,
    http: reqwest::Client,
}

#[derive(Deserialize)]
struct CamimBoleto {
    id: i64,
    posto_letra: Option<String>, // the letter we send as posto
    status: Option<String>,      // e.g. APROVADO, FIN_RECEBIDO
    situacao: Option<String>,    // human-readable status
    tem_nota_fiscal: Option<bool>,
}

/// Boletos of the query response: { competencia, boletos: [...] }.
fn boletos_of(raw: &str) -> Vec<CamimBoleto> {
    #[derive(Deserialize)]
    struct Response {
        #[serde(default)]
        boletos: Vec<serde_json::Value>,
    }
    serde_json::from_str::<Response>(raw)
        .map(|r| {
            r.boletos
                .into_iter()
                .filter_map(|b| serde_json::from_value(b).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Same characters JavaScript's encodeURIComponent escapes.
const COMPONENT: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'!')
    .remove(b'~')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')');

fn encode(v: &str) -> String {
    percent_encoding::utf8_percent_encode(v, COMPONENT).to_string()
}

impl CamimDeliveryProvider {
    pub fn new(base_url: &str, token: Option<String>) -> Self {
        Self {
            base_url: base_url.to_string(),
            token: token.filter(|t| !t.is_empty()),
            http: reqwest::Client::new(),
        }
    }

    fn url(&self) -> String {
        format!("{}/boletos/", self.base_url.trim_end_matches('/'))
    }

    fn bearer(&self) -> Result<String> {
        self.token
            .as_ref()
            .map(|t| format!("Bearer {t}"))
            .ok_or_else(|| AppError::internal("Camim: CAMIM_TOKEN não configurado"))
    }

    async fn call(&self, req: reqwest::RequestBuilder) -> Result<String> {
        let res = req
            .send()
            .await
            .map_err(|e| AppError::internal(format!("Camim: {e}")))?;
        let status = res.status();
        let raw = res.text().await.unwrap_or_default();
        if !status.is_success() {
            let head: String = raw.chars().take(300).collect();
            return Err(AppError::internal(format!(
                "Camim: HTTP {} {head}",
                status.as_u16()
            )));
        }
        Ok(raw)
    }
}

#[async_trait]
impl DeliveryProvider for CamimDeliveryProvider {
    fn label(&self) -> &str {
        "Camim"
    }

    fn configured(&self) -> bool {
        self.token.is_some()
    }

    fn required_fields(&self) -> Vec<&'static str> {
        vec!["station"]
    }

    /// The month's query lists the boletos already there, one per posto. The
    /// pair counts as sent when there is one for the customer's posto
    /// (posto_letra); an NFS-e alone also needs that boleto to already have its
    /// nota fiscal.
    async fn already_sent(
        &self,
        customer: &BillingParty,
        competence: &str,
        parts: DeliveryParts,
    ) -> Result<AlreadySent> {
        let bearer = self.bearer()?;
        let raw = self
            .call(
                self.http
                    .get(format!("{}?competencia={}", self.url(), encode(competence)))
                    .header("authorization", bearer),
            )
            .await?;
        let station = customer
            .station
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(str::to_uppercase);
        let item = station.and_then(|station| {
            boletos_of(&raw).into_iter().find(|i| {
                i.posto_letra
                    .as_deref()
                    .is_some_and(|l| l.trim().to_uppercase() == station)
            })
        });
        let Some(item) =
            item.filter(|i| !(parts == DeliveryParts::Nfse && i.tem_nota_fiscal == Some(false)))
        else {
            return Ok(AlreadySent {
                sent: false,
                raw,
                detail: None,
            });
        };
        let mut detail = vec![format!("id {}", item.id)];
        if let Some(s) = item.situacao.or(item.status).filter(|s| !s.is_empty()) {
            detail.push(s);
        }
        if item.tem_nota_fiscal == Some(false) {
            detail.push("sem nota fiscal".into());
        }
        Ok(AlreadySent {
            sent: true,
            raw,
            detail: Some(detail.join(", ")),
        })
    }

    async fn send(&self, req: &DeliveryRequest) -> Result<String> {
        let Some(station) = req.customer.station.as_deref().filter(|s| !s.is_empty()) else {
            return Err(AppError::internal("Camim: cliente sem posto"));
        };
        if req.slip_pdf.is_none() && req.nfse_pdf.is_none() {
            return Err(AppError::internal("Camim: nada para enviar"));
        }
        let bearer = self.bearer()?;
        let pdf = |bytes: &[u8], name: String| {
            Part::bytes(bytes.to_vec())
                .file_name(name)
                .mime_str("application/pdf")
                .map_err(AppError::from)
        };
        let mut form = Form::new().text("posto", station.to_string());
        if let Some(slip) = &req.slip_pdf {
            form = form.part(
                "arquivo",
                pdf(slip, format!("boleto-{}.pdf", req.competence))?,
            );
        }
        if let Some(nfse) = &req.nfse_pdf {
            form = form.part(
                "nota_fiscal",
                pdf(nfse, format!("nfse-{}.pdf", req.competence))?,
            );
        }
        // Both files: the pair route. One file alone: its own route under the
        // competence.
        let mut url = self.url();
        if req.slip_pdf.is_some() && req.nfse_pdf.is_some() {
            form = form.text("competencia", req.competence.clone());
        } else {
            let kind = if req.slip_pdf.is_some() {
                "boleto"
            } else {
                "nota"
            };
            url += &format!("{}/{kind}/", encode(&req.competence));
        }
        self.call(
            self.http
                .post(url)
                .header("authorization", bearer)
                .multipart(form),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeHttp;

    fn customer(station: Option<&str>) -> BillingParty {
        BillingParty {
            name: "C".into(),
            cnpj: "11222333000181".into(),
            address: "Rua A".into(),
            address_number: "1".into(),
            neighborhood: "Centro".into(),
            city: "Rio".into(),
            state: "RJ".into(),
            zipcode: "20000000".into(),
            station: station.map(str::to_string),
            ..BillingParty::default()
        }
    }

    fn provider(http: &FakeHttp) -> CamimDeliveryProvider {
        CamimDeliveryProvider::new(&format!("{}/api/", http.url()), Some("t".into()))
    }

    fn request(nfse: bool, slip: bool) -> DeliveryRequest {
        DeliveryRequest {
            customer: customer(Some("G")),
            competence: "2026-09".into(),
            nfse_pdf: nfse.then(|| b"nf".to_vec()),
            slip_pdf: slip.then(|| b"boleto".to_vec()),
        }
    }

    #[tokio::test]
    async fn posts_competence_boleto_and_nfse_as_multipart_with_the_bearer_token() {
        let http = FakeHttp::start(201, r#"{"id":1}"#).await;
        let raw = provider(&http).send(&request(true, true)).await.unwrap();
        assert_eq!(raw, r#"{"id":1}"#);
        let req = http.last();
        assert!(req.starts_with("POST /api/boletos/ HTTP/1.1"));
        assert!(req.to_lowercase().contains("authorization: bearer t"));
        assert!(req.contains("name=\"competencia\"\r\n\r\n2026-09"));
        assert!(req.contains("name=\"posto\"\r\n\r\nG"));
        assert!(req.contains("name=\"arquivo\"; filename=\"boleto-2026-09.pdf\""));
        assert!(req.contains("name=\"nota_fiscal\"; filename=\"nfse-2026-09.pdf\""));
        assert!(req.contains("\r\n\r\nboleto\r\n") && req.contains("\r\n\r\nnf\r\n"));
    }

    #[tokio::test]
    async fn posts_only_the_document_that_was_chosen() {
        let http = FakeHttp::start(201, "{}").await;
        let p = provider(&http);
        p.send(&request(true, false)).await.unwrap();
        let req = http.last();
        assert!(req.starts_with("POST /api/boletos/2026-09/nota/ "));
        assert!(!req.contains("name=\"arquivo\"") && req.contains("name=\"nota_fiscal\""));
        assert!(!req.contains("name=\"competencia\""));
        p.send(&request(false, true)).await.unwrap();
        let req = http.last();
        assert!(req.starts_with("POST /api/boletos/2026-09/boleto/ "));
        assert!(req.contains("name=\"arquivo\"") && !req.contains("name=\"nota_fiscal\""));
        let e = p.send(&request(false, false)).await.unwrap_err();
        assert!(e.message.contains("nada para enviar"));
    }

    #[tokio::test]
    async fn an_nfse_alone_is_not_sent_yet_when_the_boleto_there_has_no_nota() {
        let item = |nota: bool| {
            format!(r#"{{"boletos":[{{"id":1,"posto_letra":"G","tem_nota_fiscal":{nota}}}]}}"#)
        };
        let c = customer(Some("G"));
        let http = FakeHttp::start(200, &item(false)).await;
        let p = provider(&http);
        assert!(
            !p.already_sent(&c, "2026-09", DeliveryParts::Nfse)
                .await
                .unwrap()
                .sent
        );
        assert!(
            p.already_sent(&c, "2026-09", DeliveryParts::Slip)
                .await
                .unwrap()
                .sent
        );
        let http = FakeHttp::start(200, &item(true)).await;
        let p = provider(&http);
        assert!(
            p.already_sent(&c, "2026-09", DeliveryParts::Nfse)
                .await
                .unwrap()
                .sent
        );
    }

    #[tokio::test]
    async fn refuses_to_send_without_the_customers_posto() {
        let http = FakeHttp::start(200, "{}").await;
        let mut req = request(true, true);
        req.customer.station = None;
        let e = provider(&http).send(&req).await.unwrap_err();
        assert!(e.message.contains("sem posto"));
        assert_eq!(http.count(), 0);
    }

    #[tokio::test]
    async fn queries_by_competence_and_only_counts_the_customers_posto_as_sent() {
        let boleto = |id: i64, letra: Option<&str>, nota: bool| {
            serde_json::json!({
                "id": id,
                "competencia": "2026-09",
                "posto": letra.map(|l| format!("{l} Campo Grande")),
                "posto_letra": letra,
                "status": "APROVADO",
                "situacao": "Valor confere — enviado p/ pagamento",
                "valor_esperado": "1712.50",
                "tem_nota_fiscal": nota,
            })
        };
        let c = customer(Some("G"));
        let body = serde_json::json!({
            "competencia": "2026-09",
            "boletos": [boleto(38, None, true), boleto(97, Some("A"), true), boleto(98, Some("g"), false)],
        });
        let http = FakeHttp::start(200, &body.to_string()).await;
        let r = provider(&http)
            .already_sent(&c, "2026-09", DeliveryParts::Both)
            .await
            .unwrap();
        assert!(r.sent);
        assert_eq!(
            r.detail.as_deref(),
            Some("id 98, Valor confere — enviado p/ pagamento, sem nota fiscal")
        );
        assert!(http
            .last()
            .starts_with("GET /api/boletos/?competencia=2026-09 "));

        // other postos (or none) don't count, nor does the name alone
        let mut named = boleto(100, None, true);
        named["posto"] = "G".into();
        let body = serde_json::json!({ "boletos": [boleto(38, None, true), boleto(99, Some("X"), true), named] });
        let http = FakeHttp::start(200, &body.to_string()).await;
        assert!(
            !provider(&http)
                .already_sent(&c, "2026-09", DeliveryParts::Both)
                .await
                .unwrap()
                .sent
        );
        let http = FakeHttp::start(200, r#"{"competencia":"2026-09","boletos":[]}"#).await;
        assert!(
            !provider(&http)
                .already_sent(&c, "2026-09", DeliveryParts::Both)
                .await
                .unwrap()
                .sent
        );
    }

    #[tokio::test]
    async fn fails_on_http_errors_and_without_a_token() {
        let c = customer(Some("G"));
        let http = FakeHttp::start(401, "nope").await;
        let e = provider(&http)
            .already_sent(&c, "2026-09", DeliveryParts::Both)
            .await
            .unwrap_err();
        assert!(e.message.contains("HTTP 401"));
        let p = CamimDeliveryProvider::new(&http.url(), None);
        assert!(!p.configured());
        let e = p
            .already_sent(&c, "2026-09", DeliveryParts::Both)
            .await
            .unwrap_err();
        assert!(e.message.contains("CAMIM_TOKEN"));
    }
}
