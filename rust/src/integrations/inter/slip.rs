use super::api::{cents, text, InterApi};
use crate::error::{AppError, Result};
use crate::integrations::types::{ImportedSlip, SlipPayer, SlipProvider, SlipRequest, SlipResult};
use async_trait::async_trait;
use base64::Engine;
use reqwest::Method;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

const SCOPE: &str = "boleto-cobranca.read boleto-cobranca.write";

pub struct InterSlipProvider {
    inter: Arc<InterApi>,
}

fn opt(v: &Value) -> Option<String> {
    text(v).filter(|s| !s.is_empty())
}

impl InterSlipProvider {
    pub fn new(inter: Arc<InterApi>) -> Self {
        Self { inter }
    }

    async fn api(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        self.inter.call(SCOPE, method, path, body).await
    }

    async fn pdf(&self, codigo: &str) -> Result<Vec<u8>> {
        let r = self
            .api(
                Method::GET,
                &format!("/cobranca/v3/cobrancas/{codigo}/pdf"),
                None,
            )
            .await?;
        let b64 = r["pdf"]
            .as_str()
            .ok_or_else(|| AppError::internal("Banco Inter: resposta sem o PDF do boleto"))?;
        Ok(base64::engine::general_purpose::STANDARD.decode(b64)?)
    }
}

#[async_trait]
impl SlipProvider for InterSlipProvider {
    async fn list(
        &self,
        from: &str,
        to: &str,
        known: &HashSet<String>,
    ) -> Result<Vec<ImportedSlip>> {
        let mut slips = Vec::new();
        let mut pages = 1;
        let mut page = 0;
        while page < pages {
            let res = self
                .api(
                    Method::GET,
                    &format!(
                        "/cobranca/v3/cobrancas?dataInicial={from}&dataFinal={to}\
                         &filtrarDataPor=VENCIMENTO&paginaAtual={page}"
                    ),
                    None,
                )
                .await?;
            pages = res["totalPaginas"].as_u64().unwrap_or(1);
            for item in res["cobrancas"].as_array().into_iter().flatten() {
                let (c, b) = (&item["cobranca"], &item["boleto"]);
                if c["situacao"] == "CANCELADO" {
                    continue;
                }
                let codigo = text(&c["codigoSolicitacao"]).unwrap_or_default();
                // On failure the boleto is imported without PDF.
                let pdf = if known.contains(&codigo) {
                    None
                } else {
                    self.pdf(&codigo).await.ok()
                };
                let p = &c["pagador"];
                slips.push(ImportedSlip {
                    codigo_solicitacao: codigo,
                    nosso_numero: opt(&b["nossoNumero"]),
                    linha_digitavel: opt(&b["linhaDigitavel"]),
                    barcode: opt(&b["codigoBarras"]),
                    status: opt(&c["situacao"]),
                    due_date: text(&c["dataVencimento"]).unwrap_or_default(),
                    amount_cents: cents(&c["valorNominal"]),
                    pdf,
                    payer: SlipPayer {
                        tax_id: text(&p["cpfCnpj"]).unwrap_or_default(),
                        name: text(&p["nome"]).unwrap_or_default(),
                        address: opt(&p["endereco"]),
                        number: opt(&p["numero"]),
                        complement: opt(&p["complemento"]),
                        neighborhood: opt(&p["bairro"]),
                        city: opt(&p["cidade"]),
                        state: opt(&p["uf"]),
                        zipcode: opt(&p["cep"]),
                    },
                });
            }
            page += 1;
        }
        Ok(slips)
    }

    async fn cancel(&self, codigo_solicitacao: &str, reason: &str) -> Result<()> {
        self.api(
            Method::POST,
            &format!("/cobranca/v3/cobrancas/{codigo_solicitacao}/cancelar"),
            Some(json!({ "motivoCancelamento": reason })),
        )
        .await?;
        Ok(())
    }

    async fn create(&self, req: &SlipRequest) -> Result<SlipResult> {
        let c = &req.customer;
        let id = &req.billing_id;
        let created = self
            .api(
                Method::POST,
                "/cobranca/v3/cobrancas",
                Some(json!({
                    "seuNumero": &id[id.len().saturating_sub(15)..],
                    "valorNominal": req.amount_cents as f64 / 100.0,
                    "dataVencimento": req.due_date,
                    "numDiasAgenda": 30,
                    "pagador": {
                        "cpfCnpj": c.cnpj,
                        "tipoPessoa": "JURIDICA",
                        "nome": c.name,
                        "endereco": c.address,
                        "numero": c.address_number,
                        "complemento": c.address_complement.clone().unwrap_or_default(),
                        "bairro": c.neighborhood,
                        "cidade": c.city,
                        "uf": c.state,
                        "cep": c.zipcode,
                    },
                })),
            )
            .await?;
        let codigo = text(&created["codigoSolicitacao"]).ok_or_else(|| {
            AppError::internal(format!(
                "Banco Inter: resposta sem código de solicitação: {created}"
            ))
        })?;

        // The boleto is registered asynchronously; poll until it has a barcode.
        let mut detail = Value::Null;
        for _ in 0..10 {
            detail = self
                .api(
                    Method::GET,
                    &format!("/cobranca/v3/cobrancas/{codigo}"),
                    None,
                )
                .await?;
            if opt(&detail["boleto"]["linhaDigitavel"]).is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
        let Some(linha) = opt(&detail["boleto"]["linhaDigitavel"]) else {
            return Err(AppError::internal(format!(
                "Banco Inter: boleto {codigo} ainda em processamento; tente novamente"
            )));
        };
        let pdf = self.pdf(&codigo).await?;
        Ok(SlipResult {
            codigo_solicitacao: codigo,
            nosso_numero: opt(&detail["boleto"]["nossoNumero"]),
            linha_digitavel: Some(linha),
            barcode: opt(&detail["boleto"]["codigoBarras"]),
            status: opt(&detail["cobranca"]["situacao"]),
            pdf: Some(pdf),
        })
    }
}
