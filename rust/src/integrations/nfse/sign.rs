use super::pfx::PfxMaterial;
use super::xml::El;
use crate::error::{AppError, Result};
use base64::Engine;
use rsa::Pkcs1v15Sign;
use sha2::{Digest, Sha256};

const DSIG_NS: &str = "http://www.w3.org/2000/09/xmldsig#";
const C14N: &str = "http://www.w3.org/TR/2001/REC-xml-c14n-20010315";
const ENVELOPED: &str = "http://www.w3.org/2000/09/xmldsig#enveloped-signature";
const RSA_SHA256: &str = "http://www.w3.org/2001/04/xmldsig-more#rsa-sha256";
const SHA256: &str = "http://www.w3.org/2001/04/xmlenc#sha256";

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Enveloped XMLDSig (RSA-SHA256) over infDPS, referenced by its Id, appended
/// to `<DPS>`. Returns the signed document.
pub fn sign_dps(mut dps: El, m: &PfxMaterial) -> Result<String> {
    let inf = dps
        .find("infDPS")
        .ok_or_else(|| AppError::internal("DPS sem infDPS"))?;
    let id = inf
        .attrs
        .iter()
        .find(|(k, _)| k == "Id")
        .map(|(_, v)| v.clone())
        .ok_or_else(|| AppError::internal("infDPS sem Id"))?;
    // The signature does not exist yet, so the enveloped transform has nothing
    // to remove: the digest is of the canonical infDPS.
    let digest = Sha256::digest(inf.canonical(dps.ns.as_deref()).as_bytes());

    let algorithm = |name: &str, uri: &str| El::new(name).attr("Algorithm", uri);
    let signed_info = El::new("SignedInfo")
        .child(algorithm("CanonicalizationMethod", C14N))
        .child(algorithm("SignatureMethod", RSA_SHA256))
        .child(
            El::new("Reference")
                .attr("URI", format!("#{id}"))
                .child(
                    El::new("Transforms")
                        .child(algorithm("Transform", ENVELOPED))
                        .child(algorithm("Transform", C14N)),
                )
                .child(algorithm("DigestMethod", SHA256))
                .child(El::leaf("DigestValue", b64(&digest))),
        );
    let hashed = Sha256::digest(signed_info.canonical(Some(DSIG_NS)).as_bytes());
    let signature = m
        .key
        .sign(Pkcs1v15Sign::new::<Sha256>(), &hashed)
        .map_err(|e| AppError::internal(format!("Falha ao assinar a DPS: {e}")))?;

    let certificate = m
        .chain_der
        .first()
        .ok_or_else(|| AppError::internal("Certificado não encontrado"))?;
    dps.children.push(super::xml::Node::El(
        El::new("Signature")
            .ns(DSIG_NS)
            .child(signed_info)
            .child(El::leaf("SignatureValue", b64(&signature)))
            .child(
                El::new("KeyInfo").child(
                    El::new("X509Data").child(El::leaf("X509Certificate", b64(certificate))),
                ),
            ),
    ));
    Ok(dps.to_xml())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::nfse::dps::{build_dps, tests as dps};
    use crate::integrations::nfse::pfx::load_pfx;
    use serde_json::Value;

    // Produced by the server's buildDpsXml + signDps (xml-crypto) with a
    // throwaway key. PKCS#1 v1.5 is deterministic, so the output must match.
    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../../tests/fixtures/dps-signed.json")).unwrap()
    }

    fn decode(v: &Value) -> Vec<u8> {
        base64::engine::general_purpose::STANDARD
            .decode(v.as_str().unwrap())
            .unwrap()
    }

    #[test]
    fn builds_the_same_dps_as_the_server() {
        let f = fixture();
        let xml = build_dps(&dps::cfg(), &dps::req()).unwrap().to_xml();
        assert_eq!(xml, f["unsigned"].as_str().unwrap());
    }

    #[test]
    fn signs_exactly_like_the_server() {
        let f = fixture();
        let material = PfxMaterial::from_pems(
            f["privateKeyPem"].as_str().unwrap(),
            decode(&f["certDerB64"]),
        );
        let signed = sign_dps(build_dps(&dps::cfg(), &dps::req()).unwrap(), &material).unwrap();
        assert_eq!(signed, f["signed"].as_str().unwrap());
    }

    #[test]
    fn signs_the_same_with_the_key_read_from_a_pfx() {
        let f = fixture();
        let material = load_pfx(&decode(&f["pfxB64"]), "senha").unwrap();
        assert_eq!(material.chain_der, vec![decode(&f["certDerB64"])]);
        assert!(material.identity_pem().contains("BEGIN CERTIFICATE"));
        let signed = sign_dps(build_dps(&dps::cfg(), &dps::req()).unwrap(), &material).unwrap();
        assert_eq!(signed, f["signed"].as_str().unwrap());
        let wrong = load_pfx(&decode(&f["pfxB64"]), "errada").err().unwrap();
        assert!(wrong.message.contains("PFX inválido"));
    }
}
