//! Test doubles shared by the unit tests.

use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Local HTTP server answering every request with the same status and body,
/// keeping what it received.
pub struct FakeHttp {
    port: u16,
    requests: Arc<Mutex<Vec<String>>>,
}

fn complete(buf: &[u8]) -> bool {
    let text = String::from_utf8_lossy(buf);
    let Some(head_end) = text.find("\r\n\r\n") else {
        return false;
    };
    let head = text[..head_end].to_lowercase();
    if head.contains("transfer-encoding: chunked") {
        return text.ends_with("0\r\n\r\n");
    }
    let length = head
        .lines()
        .find_map(|l| l.strip_prefix("content-length:"))
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(0);
    buf.len() >= head_end + 4 + length
}

impl FakeHttp {
    pub async fn start(status: u16, body: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        let body = body.to_string();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 8192];
                while !complete(&buf) {
                    match socket.read(&mut chunk).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                seen.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&buf).into_owned());
                let response = format!(
                    "HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        });
        Self { port, requests }
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }

    pub fn last(&self) -> String {
        self.requests
            .lock()
            .unwrap()
            .last()
            .cloned()
            .unwrap_or_default()
    }
}

// ------------------------------------------------------------ fake providers

use crate::db::customers::{self, Customer};
use crate::db::Db;
use crate::error::{AppError, Result};
use crate::integrations::types::*;
use crate::services::import::CityLookup;
use async_trait::async_trait;
use std::collections::HashSet;

/// Registers a customer with the given name, CNPJ and boleto preference.
pub fn customer(db: &Db, name: &str, cnpj: &str, has_bank_slip: bool) -> Customer {
    let mut input = customers::sample_input(cnpj);
    input.name = name.into();
    input.has_bank_slip = has_bank_slip;
    input.recurring_value = 10000;
    db.with(|c| {
        input.validate()?;
        customers::create(c, &input)
    })
    .unwrap()
}

type Issue = Box<dyn Fn(&NfseRequest) -> Result<NfseResult> + Send + Sync>;

pub struct FakeNfse {
    pub issue: Issue,
    pub notes: Vec<ImportedNfse>,
}

impl FakeNfse {
    pub fn issuing(
        issue: impl Fn(&NfseRequest) -> Result<NfseResult> + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            issue: Box::new(issue),
            notes: vec![],
        })
    }
}

#[async_trait]
impl NfseProvider for FakeNfse {
    async fn issue(&self, req: &NfseRequest) -> Result<NfseResult> {
        (self.issue)(req)
    }

    async fn list(&self) -> Result<Vec<ImportedNfse>> {
        Ok(self.notes.clone())
    }
}

type Create = Box<dyn Fn(&SlipRequest) -> Result<SlipResult> + Send + Sync>;
type Cancel = Box<dyn Fn(&str) -> Result<()> + Send + Sync>;

pub struct FakeSlip {
    pub create: Create,
    pub cancel: Cancel,
    pub slips: Vec<ImportedSlip>,
}

impl FakeSlip {
    pub fn creating(
        create: impl Fn(&SlipRequest) -> Result<SlipResult> + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            create: Box::new(create),
            cancel: Box::new(|_| Ok(())),
            slips: vec![],
        })
    }
}

#[async_trait]
impl SlipProvider for FakeSlip {
    async fn list(&self, _: &str, _: &str, _: &HashSet<String>) -> Result<Vec<ImportedSlip>> {
        Ok(self.slips.clone())
    }

    async fn create(&self, req: &SlipRequest) -> Result<SlipResult> {
        (self.create)(req)
    }

    async fn cancel(&self, codigo_solicitacao: &str, _reason: &str) -> Result<()> {
        (self.cancel)(codigo_solicitacao)
    }
}

/// What a delivery carried.
#[derive(Debug, Clone, PartialEq)]
pub struct Sent {
    pub competence: String,
    pub nfse: bool,
    pub slip: bool,
}

pub struct FakeDelivery {
    pub configured: bool,
    /// Called on every send; Some(error) makes it fail.
    pub fail: Option<Box<dyn Fn() -> Option<AppError> + Send + Sync>>,
    pub sent: Arc<Mutex<Vec<Sent>>>,
}

impl Default for FakeDelivery {
    fn default() -> Self {
        Self {
            configured: true,
            fail: None,
            sent: Arc::default(),
        }
    }
}

#[async_trait]
impl DeliveryProvider for FakeDelivery {
    fn label(&self) -> &str {
        "Camim"
    }

    fn configured(&self) -> bool {
        self.configured
    }

    fn required_fields(&self) -> Vec<&'static str> {
        vec!["station"]
    }

    async fn already_sent(
        &self,
        _: &BillingParty,
        _: &str,
        _: DeliveryParts,
    ) -> Result<AlreadySent> {
        Ok(AlreadySent {
            sent: false,
            raw: "[]".into(),
            detail: None,
        })
    }

    async fn send(&self, req: &DeliveryRequest) -> Result<String> {
        if let Some(e) = self.fail.as_ref().and_then(|f| f()) {
            return Err(e);
        }
        self.sent.lock().unwrap().push(Sent {
            competence: req.competence.clone(),
            nfse: req.nfse_pdf.is_some(),
            slip: req.slip_pdf.is_some(),
        });
        Ok(r#"{"ok":true}"#.into())
    }
}

pub struct NoCities;

#[async_trait]
impl CityLookup for NoCities {
    async fn city_of(&self, _code: &str) -> (String, String) {
        ("-".into(), "-".into())
    }
}
