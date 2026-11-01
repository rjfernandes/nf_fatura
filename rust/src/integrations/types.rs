use crate::error::Result;
use async_trait::async_trait;
use serde::Serialize;
use std::collections::HashSet;

/// Who the NFS-e and the boleto are issued to.
#[derive(Debug, Clone, Default)]
pub struct BillingParty {
    pub name: String,
    pub cnpj: String,
    pub address: String,
    pub address_number: String,
    pub address_complement: Option<String>,
    pub neighborhood: String,
    pub city: String,
    pub city_ibge_code: Option<String>,
    pub state: String,
    pub zipcode: String,
    pub station: Option<String>, // posto
}

#[derive(Debug, Clone)]
pub struct NfseRequest {
    pub billing_id: String,
    pub dps_number: i64, // sequential nDPS, stable across retries
    pub customer: BillingParty,
    pub amount_cents: i64,
    pub competence: String, // YYYY-MM
}

#[derive(Debug, Clone, Default)]
pub struct NfseResult {
    pub number: Option<String>,
    pub access_key: Option<String>,
    pub xml: Option<String>,
    pub pdf: Option<Vec<u8>>,
    pub raw: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TakerAddress {
    pub address: Option<String>,
    pub number: Option<String>,
    pub complement: Option<String>,
    pub neighborhood: Option<String>,
    pub city_ibge_code: Option<String>,
    pub zipcode: Option<String>,
}

/// An NFS-e already issued by the company, read back from the portal.
#[derive(Debug, Clone, Default)]
pub struct ImportedNfse {
    pub access_key: String,
    pub number: String,
    pub xml: String,
    pub issued_at: String, // dhEmi as written in the XML, e.g. 2026-09-30T23:50:00-03:00
    pub competence: String, // YYYY-MM, the note's own competence (dCompet)
    pub taker_tax_id: String,
    pub taker_name: Option<String>,
    pub taker_address: TakerAddress,
    pub amount_cents: i64,
}

#[async_trait]
pub trait NfseProvider: Send + Sync {
    async fn issue(&self, req: &NfseRequest) -> Result<NfseResult>;
    /// NFS-e issued by the company, as the portal's DF-e distribution lists them.
    async fn list(&self) -> Result<Vec<ImportedNfse>>;
}

#[derive(Debug, Clone)]
pub struct SlipRequest {
    pub billing_id: String,
    pub customer: BillingParty,
    pub amount_cents: i64,
    pub due_date: String, // YYYY-MM-DD
}

#[derive(Debug, Clone, Default)]
pub struct SlipResult {
    pub codigo_solicitacao: String,
    pub nosso_numero: Option<String>,
    pub linha_digitavel: Option<String>,
    pub barcode: Option<String>,
    pub pdf: Option<Vec<u8>>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SlipPayer {
    pub tax_id: String,
    pub name: String,
    pub address: Option<String>,
    pub number: Option<String>,
    pub complement: Option<String>,
    pub neighborhood: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub zipcode: Option<String>,
}

/// A boleto already registered at the bank.
#[derive(Debug, Clone, Default)]
pub struct ImportedSlip {
    pub codigo_solicitacao: String,
    pub nosso_numero: Option<String>,
    pub linha_digitavel: Option<String>,
    pub barcode: Option<String>,
    pub status: Option<String>,
    pub due_date: String, // YYYY-MM-DD
    pub amount_cents: i64,
    pub pdf: Option<Vec<u8>>,
    pub payer: SlipPayer,
}

#[async_trait]
pub trait SlipProvider: Send + Sync {
    /// Boletos due between two dates (YYYY-MM-DD), cancelled ones left out. The
    /// PDF is only downloaded for codes not in `known`.
    async fn list(
        &self,
        from: &str,
        to: &str,
        known: &HashSet<String>,
    ) -> Result<Vec<ImportedSlip>>;
    async fn create(&self, req: &SlipRequest) -> Result<SlipResult>;
    /// Cancels the boleto at the bank; fails if the bank refuses (e.g. already
    /// paid).
    async fn cancel(&self, codigo_solicitacao: &str, reason: &str) -> Result<()>;
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatementEntry {
    pub id: String,
    pub date: String, // YYYY-MM-DD
    // full timestamp when the bank provides one (orders entries within a day),
    // else the date
    pub at: String,
    #[serde(rename = "type")]
    pub kind: String, // e.g. PIX, BOLETO_COBRANCA
    pub operation: String, // C crédito | D débito
    pub amount_cents: i64, // always positive; see operation
    pub title: String,
    pub description: String,
    pub counterpart: String, // payer/recipient name, when the bank provides it
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StatementBalance {
    pub available: i64, // cents
    pub blocked: i64,   // cents
}

#[async_trait]
pub trait StatementProvider: Send + Sync {
    /// Entries between two dates, inclusive (YYYY-MM-DD).
    async fn fetch(&self, from: &str, to: &str) -> Result<Vec<StatementEntry>>;
    /// Balance at the end of the given day (YYYY-MM-DD).
    async fn balance(&self, date: &str) -> Result<StatementBalance>;
}

/// Customer fields an integration may require, with their display names.
pub const DELIVERY_FIELDS: [(&str, &str); 1] = [("station", "posto")];

pub fn delivery_field_label(field: &str) -> &str {
    DELIVERY_FIELDS
        .iter()
        .find(|(f, _)| *f == field)
        .map_or(field, |(_, label)| label)
}

/// What goes to the customer: the NFS-e, the boleto or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryParts {
    Nfse,
    Slip,
    Both,
}

impl DeliveryParts {
    pub fn parse(v: &str) -> Option<Self> {
        match v {
            "NFSE" => Some(Self::Nfse),
            "SLIP" => Some(Self::Slip),
            "BOTH" => Some(Self::Both),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Nfse => "NFSE",
            Self::Slip => "SLIP",
            Self::Both => "BOTH",
        }
    }
}

pub struct DeliveryRequest {
    pub customer: BillingParty,
    pub competence: String,        // YYYY-MM
    pub nfse_pdf: Option<Vec<u8>>, // present when the NFS-e is part of the delivery
    pub slip_pdf: Option<Vec<u8>>, // present when the boleto is part of the delivery
}

#[derive(Debug)]
pub struct AlreadySent {
    pub sent: bool,
    pub raw: String,
    pub detail: Option<String>,
}

/// Sends the NFS-e, the boleto or both to the customer's own system.
#[async_trait]
pub trait DeliveryProvider: Send + Sync {
    fn label(&self) -> &str;
    fn configured(&self) -> bool;
    /// Customer fields that must be filled for this integration to work.
    fn required_fields(&self) -> Vec<&'static str>;
    /// Whether what is being sent for the competence is already there; detail
    /// describes what was found (status, missing NFS-e...).
    async fn already_sent(
        &self,
        customer: &BillingParty,
        competence: &str,
        parts: DeliveryParts,
    ) -> Result<AlreadySent>;
    /// Returns the raw response.
    async fn send(&self, req: &DeliveryRequest) -> Result<String>;
}
