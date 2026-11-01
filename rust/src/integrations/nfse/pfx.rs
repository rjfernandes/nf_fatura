use crate::error::{AppError, Result};
use base64::Engine;
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::RsaPrivateKey;

/// Key and certificates of an A1 certificate (.pfx).
pub struct PfxMaterial {
    pub key: RsaPrivateKey,
    key_pkcs8_der: Vec<u8>,
    /// The company's certificate first, then its issuers.
    pub chain_der: Vec<Vec<u8>>,
}

fn invalid(detail: impl std::fmt::Display) -> AppError {
    AppError::internal(format!("PFX inválido: {detail}"))
}

fn parse_key(der: &[u8]) -> Result<RsaPrivateKey> {
    RsaPrivateKey::from_pkcs8_der(der)
        .or_else(|_| RsaPrivateKey::from_pkcs1_der(der))
        .map_err(|_| invalid("a chave não é RSA"))
}

pub fn load_pfx(pfx: &[u8], password: &str) -> Result<PfxMaterial> {
    let store = p12_keystore::KeyStore::from_pkcs12(pfx, password)
        .map_err(|e| invalid(format!("senha incorreta ou arquivo corrompido ({e})")))?;
    let (_, chain) = store
        .private_key_chain()
        .ok_or_else(|| invalid("chave ou certificado não encontrados"))?;
    let key = parse_key(chain.key())?;
    let chain_der: Vec<Vec<u8>> = chain.chain().iter().map(|c| c.as_der().to_vec()).collect();
    if chain_der.is_empty() {
        return Err(invalid("chave ou certificado não encontrados"));
    }
    Ok(PfxMaterial {
        key,
        key_pkcs8_der: chain.key().to_vec(),
        chain_der,
    })
}

fn pem(label: &str, der: &[u8]) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(der);
    let lines: Vec<&str> = b64
        .as_bytes()
        .chunks(64)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect();
    format!(
        "-----BEGIN {label}-----\n{}\n-----END {label}-----\n",
        lines.join("\n")
    )
}

impl PfxMaterial {
    #[cfg(test)]
    pub fn from_pems(key_pem: &str, cert_der: Vec<u8>) -> Self {
        use rsa::pkcs8::EncodePrivateKey;
        let key = RsaPrivateKey::from_pkcs1_pem(key_pem)
            .or_else(|_| RsaPrivateKey::from_pkcs8_pem(key_pem))
            .unwrap();
        let key_pkcs8_der = key.to_pkcs8_der().unwrap().as_bytes().to_vec();
        Self {
            key,
            key_pkcs8_der,
            chain_der: vec![cert_der],
        }
    }

    /// Certificates and key as one PEM bundle, for the TLS client identity.
    pub fn identity_pem(&self) -> String {
        let mut out: String = self
            .chain_der
            .iter()
            .map(|c| pem("CERTIFICATE", c))
            .collect();
        // The stored key may be PKCS#1 when the PFX carried it that way.
        let label = if RsaPrivateKey::from_pkcs8_der(&self.key_pkcs8_der).is_ok() {
            "PRIVATE KEY"
        } else {
            "RSA PRIVATE KEY"
        };
        out.push_str(&pem(label, &self.key_pkcs8_der));
        out
    }
}
