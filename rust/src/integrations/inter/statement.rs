use super::api::{cents, text, InterApi};
use crate::error::Result;
use crate::integrations::types::{StatementBalance, StatementEntry, StatementProvider};
use async_trait::async_trait;
use reqwest::Method;
use std::sync::Arc;

const SCOPE: &str = "extrato.read";
const PAGE_SIZE: u32 = 100;
const MAX_PAGES: u64 = 200;

pub struct InterStatementProvider {
    inter: Arc<InterApi>,
}

impl InterStatementProvider {
    pub fn new(inter: Arc<InterApi>) -> Self {
        Self { inter }
    }
}

#[async_trait]
impl StatementProvider for InterStatementProvider {
    async fn fetch(&self, from: &str, to: &str) -> Result<Vec<StatementEntry>> {
        let mut entries: Vec<StatementEntry> = Vec::new();
        for page in 0..MAX_PAGES {
            let res = self
                .inter
                .call(
                    SCOPE,
                    Method::GET,
                    &format!(
                        "/banking/v2/extrato/completo?dataInicio={from}&dataFim={to}\
                         &pagina={page}&tamanhoPagina={PAGE_SIZE}"
                    ),
                    None,
                )
                .await?;
            for t in res["transacoes"].as_array().into_iter().flatten() {
                let added = text(&t["dataInclusao"]);
                let moved = text(&t["dataTransacao"]);
                let d = &t["detalhes"];
                let counterpart = [
                    "nomePagador",
                    "nomeRecebedor",
                    "nomeOrigem",
                    "nomeDestino",
                    "nomeFavorecido",
                ]
                .iter()
                .find_map(|k| text(&d[*k]))
                .unwrap_or_default();
                entries.push(StatementEntry {
                    id: text(&t["idTransacao"])
                        .unwrap_or_else(|| format!("{from}-{}", entries.len())),
                    date: moved
                        .clone()
                        .or_else(|| added.as_ref().map(|a| a.chars().take(10).collect()))
                        .unwrap_or_else(|| from.to_string()),
                    at: added.or(moved).unwrap_or_else(|| from.to_string()),
                    kind: text(&t["tipoTransacao"]).unwrap_or_default(),
                    operation: if t["tipoOperacao"] == "D" { "D" } else { "C" }.into(),
                    amount_cents: cents(&t["valor"]),
                    title: text(&t["titulo"]).unwrap_or_default(),
                    description: text(&t["descricao"]).unwrap_or_default(),
                    counterpart,
                });
            }
            if res["ultimaPagina"] == true || page + 1 >= res["totalPaginas"].as_u64().unwrap_or(1)
            {
                break;
            }
        }
        entries.sort_by(|a, b| a.date.cmp(&b.date));
        Ok(entries)
    }

    async fn balance(&self, date: &str) -> Result<StatementBalance> {
        let r = self
            .inter
            .call(
                SCOPE,
                Method::GET,
                &format!("/banking/v2/saldo?dataSaldo={date}"),
                None,
            )
            .await?;
        Ok(StatementBalance {
            available: cents(&r["disponivel"]),
            blocked: cents(&r["bloqueadoCheque"])
                + cents(&r["bloqueadoJudicialmente"])
                + cents(&r["bloqueadoAdministrativo"]),
        })
    }
}
