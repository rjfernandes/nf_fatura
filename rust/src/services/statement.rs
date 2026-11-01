use crate::error::{AppError, Result};
use crate::integrations::types::{StatementEntry, StatementProvider};
use crate::util::period::{is_competence, is_future_month, month_range_until_today, today, Range};
use crate::util::statement_files::{build_ofx, build_pdf, AccountInfo, StatementFileData};
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Serialize)]
pub struct Totals {
    pub credits: i64,
    pub debits: i64,
    pub net: i64,
}

#[derive(Debug, Serialize)]
pub struct StatementSummary {
    pub competence: String,
    pub from: String,
    pub to: String,
    pub entries: Vec<StatementEntry>,
    pub totals: Totals, // cents
}

#[derive(Debug)]
pub struct StatementFile {
    pub file_name: String,
    pub content: Vec<u8>,
}

fn dmy(iso: &str) -> String {
    iso.split('-').rev().collect::<Vec<_>>().join("-")
}

/// Statement lookups and file exports.
pub struct StatementService {
    provider: Arc<dyn StatementProvider>,
    account: AccountInfo,
}

impl StatementService {
    pub fn new(provider: Arc<dyn StatementProvider>, account: AccountInfo) -> Self {
        Self { provider, account }
    }

    fn period(&self, competence: &str) -> Result<Range> {
        if !is_competence(competence) {
            return Err(AppError::new(
                400,
                "Dados inválidos: competence deve ser AAAA-MM",
            ));
        }
        if is_future_month(competence, today()) {
            return Err(AppError::new(400, "Escolha o mês atual ou um anterior"));
        }
        Ok(month_range_until_today(competence, today()))
    }

    pub async fn summary(&self, competence: &str) -> Result<StatementSummary> {
        let Range { from, to } = self.period(competence)?;
        let entries = self.provider.fetch(&from, &to).await?;
        let sum = |op: &str| -> i64 {
            entries
                .iter()
                .filter(|e| e.operation == op)
                .map(|e| e.amount_cents)
                .sum()
        };
        let (credits, debits) = (sum("C"), sum("D"));
        Ok(StatementSummary {
            competence: competence.to_string(),
            from,
            to,
            entries,
            totals: Totals {
                credits,
                debits,
                net: credits - debits,
            },
        })
    }

    /// Same layouts Banco Inter exports: PDF and OFX 1.02.
    pub async fn export(&self, competence: &str, format: &str) -> Result<StatementFile> {
        if format != "pdf" && format != "ofx" {
            return Err(AppError::new(
                400,
                "Dados inválidos: format deve ser pdf ou ofx",
            ));
        }
        let Range { from, to } = self.period(competence)?;
        let (entries, balance) =
            tokio::try_join!(self.provider.fetch(&from, &to), self.provider.balance(&to))?;
        let file_name = format!(
            "Extrato-{}-a-{}-{}.{format}",
            dmy(&from),
            dmy(&to),
            format.to_uppercase()
        );
        let data = StatementFileData {
            from,
            to,
            entries,
            available: balance.available,
            blocked: balance.blocked,
            closing_cents: balance.available + balance.blocked,
        };
        let now = chrono::Local::now().naive_local();
        Ok(StatementFile {
            file_name,
            content: if format == "ofx" {
                build_ofx(&self.account, &data, now)
            } else {
                build_pdf(&self.account, &data, now)
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::types::StatementBalance;
    use async_trait::async_trait;

    struct Fake;

    #[async_trait]
    impl StatementProvider for Fake {
        async fn fetch(&self, from: &str, _to: &str) -> Result<Vec<StatementEntry>> {
            let e = |op: &str, cents: i64| StatementEntry {
                id: format!("{op}{cents}"),
                date: from.to_string(),
                at: from.to_string(),
                kind: "PIX".into(),
                operation: op.into(),
                amount_cents: cents,
                title: "Pix".into(),
                description: String::new(),
                counterpart: String::new(),
            };
            Ok(vec![e("C", 1000), e("D", 300), e("C", 50)])
        }

        async fn balance(&self, _date: &str) -> Result<StatementBalance> {
            Ok(StatementBalance {
                available: 750,
                blocked: 0,
            })
        }
    }

    #[tokio::test]
    async fn sums_the_month_and_exports_with_the_bank_file_names() {
        let svc = StatementService::new(Arc::new(Fake), AccountInfo::default());
        let s = svc.summary("2026-08").await.unwrap();
        assert_eq!(
            (s.from.as_str(), s.to.as_str()),
            ("2026-08-01", "2026-08-31")
        );
        assert_eq!(
            (s.totals.credits, s.totals.debits, s.totals.net),
            (1050, 300, 750)
        );
        let pdf = svc.export("2026-08", "pdf").await.unwrap();
        assert_eq!(pdf.file_name, "Extrato-01-08-2026-a-31-08-2026-PDF.pdf");
        assert!(pdf.content.starts_with(b"%PDF-"));
        let ofx = svc.export("2026-08", "ofx").await.unwrap();
        assert_eq!(ofx.file_name, "Extrato-01-08-2026-a-31-08-2026-OFX.ofx");
        assert_eq!(svc.export("2026-08", "csv").await.unwrap_err().status, 400);
        assert_eq!(svc.summary("2999-01").await.unwrap_err().status, 400);
        assert_eq!(svc.summary("agosto").await.unwrap_err().status, 400);
    }
}
