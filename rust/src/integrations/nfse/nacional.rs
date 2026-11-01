use super::dps::{build_dps, DpsConfig};
use super::parse::parse_nfse;
use super::pfx::{load_pfx, PfxMaterial};
use super::sign::sign_dps;
use crate::config::Env;
use crate::error::{AppError, Result};
use crate::integrations::inter::api::describe;
use crate::integrations::types::{ImportedNfse, NfseProvider, NfseRequest, NfseResult};
use crate::util::cnpj::{normalize_cnpj, only_digits};
use async_trait::async_trait;
use base64::Engine;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Urls {
    sefin: &'static str,
    adn: &'static str,
}

fn urls(env: &str) -> Urls {
    if env == "producao" {
        Urls {
            sefin: "https://sefin.nfse.gov.br/SefinNacional",
            adn: "https://adn.nfse.gov.br",
        }
    } else {
        Urls {
            sefin: "https://sefin.producaorestrita.nfse.gov.br/SefinNacional",
            adn: "https://adn.producaorestrita.nfse.gov.br",
        }
    }
}

struct Ctx {
    cfg: DpsConfig,
    client: reqwest::Client,
    material: PfxMaterial,
}

pub struct NfseNacionalProvider {
    env: Env,
    ctx: Mutex<Option<Arc<Ctx>>>,
}

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

fn gzip_b64(text: &str) -> Result<String> {
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    gz.write_all(text.as_bytes())?;
    Ok(b64().encode(gz.finish()?))
}

fn gunzip_b64(data: &str) -> Result<String> {
    let mut out = String::new();
    GzDecoder::new(&b64().decode(data)?[..]).read_to_string(&mut out)?;
    Ok(out)
}

fn net(e: reqwest::Error) -> AppError {
    AppError::internal(format!("NFS-e: {}", describe(&e)))
}

impl NfseNacionalProvider {
    pub fn new(env: Env) -> Self {
        Self {
            env,
            ctx: Mutex::new(None),
        }
    }

    fn init(&self) -> Result<Arc<Ctx>> {
        let mut cached = self.ctx.lock().unwrap();
        if let Some(ctx) = cached.as_ref() {
            return Ok(ctx.clone());
        }
        let s = &self.env.settings;
        let password = self.env.secrets.nfse_cert_pfx_password.as_deref();
        let missing: Vec<&str> = [
            ("certificado A1 (.pfx)", s.nfse_cert_pfx_path.is_some()),
            ("senha do certificado", password.is_some()),
            ("CNPJ da empresa", s.company_cnpj.is_some()),
            (
                "código IBGE do município da empresa",
                s.company_city_ibge.is_some(),
            ),
        ]
        .iter()
        .filter(|(_, set)| !set)
        .map(|(name, _)| *name)
        .collect();
        if !missing.is_empty() {
            return Err(AppError::internal(format!(
                "NFS-e: configurações não preenchidas: {}",
                missing.join(", ")
            )));
        }
        let path = s.nfse_cert_pfx_path.as_deref().unwrap_or_default();
        let pfx = std::fs::read(path)
            .map_err(|e| AppError::internal(format!("NFS-e: não foi possível ler {path}: {e}")))?;
        let material = load_pfx(&pfx, password.unwrap_or_default())?;
        let identity = reqwest::Identity::from_pem(material.identity_pem().as_bytes())
            .map_err(|e| AppError::internal(format!("NFS-e: certificado inválido: {e}")))?;
        let cfg = DpsConfig {
            tp_amb: if s.nfse_env == "producao" { 1 } else { 2 },
            provider_cnpj: normalize_cnpj(s.company_cnpj.as_deref().unwrap_or_default()),
            provider_municipal_registration: s.company_municipal_registration.clone(),
            provider_city_ibge: s.company_city_ibge.clone().unwrap_or_default(),
            provider_phone: s.company_phone.as_deref().map(only_digits),
            provider_email: s.company_email.clone(),
            service_code: s.nfse_service_code.clone(),
            municipal_service_code: Some(s.nfse_municipal_service_code.clone()),
            nbs_code: Some(s.nfse_nbs_code.clone()),
            service_description: s.nfse_service_description.clone(),
            op_simp_nac: s.nfse_simples_nacional.clone(),
            reg_ap_trib_sn: Some(s.nfse_reg_ap_trib_sn.clone()),
            iss_rate_percent: s.nfse_iss_rate_percent,
            tot_trib_sn_percent: Some(s.nfse_tot_trib_sn_percent),
            series: Some(s.nfse_series.clone()),
            now: None,
        };
        let ctx = Arc::new(Ctx {
            cfg,
            client: reqwest::Client::builder()
                .use_rustls_tls()
                .identity(identity)
                .timeout(Duration::from_secs(90))
                .build()?,
            material,
        });
        *cached = Some(ctx.clone());
        Ok(ctx)
    }
}

#[async_trait]
impl NfseProvider for NfseNacionalProvider {
    async fn issue(&self, input: &NfseRequest) -> Result<NfseResult> {
        let ctx = self.init()?;
        let urls = urls(&self.env.settings.nfse_env);
        let req = NfseRequest {
            dps_number: input.dps_number + self.env.settings.nfse_dps_offset,
            ..input.clone()
        };
        // SEFIN rejects (E1229) a DPS without an explicit UTF-8 declaration. It is
        // added after signing; canonicalization ignores the declaration.
        let xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>{}",
            sign_dps(build_dps(&ctx.cfg, &req)?, &ctx.material)?
        );
        let res = ctx
            .client
            .post(format!("{}/nfse", urls.sefin))
            .header("content-type", "application/json")
            .header("accept", "application/json")
            .body(json!({ "dpsXmlGZipB64": gzip_b64(&xml)? }).to_string())
            .send()
            .await
            .map_err(net)?;
        let status = res.status().as_u16();
        let text = res.text().await.unwrap_or_default();
        if status >= 300 {
            return Err(AppError::internal(format!("NFS-e {status}: {text}")));
        }
        let json: Value = serde_json::from_str(&text)
            .map_err(|_| AppError::internal(format!("NFS-e: resposta inesperada: {text}")))?;
        let Some(access_key) = json["chaveAcesso"].as_str().filter(|k| !k.is_empty()) else {
            return Err(AppError::internal(format!(
                "NFS-e: resposta sem chave de acesso: {text}"
            )));
        };
        let nfse_xml = match json["nfseXmlGZipB64"].as_str() {
            Some(data) => Some(gunzip_b64(data)?),
            None => None,
        };
        let number = nfse_xml.as_deref().and_then(|x| {
            let rest = x.split_once("<nNFSe>")?.1;
            let n = rest.split_once("</nNFSe>")?.0;
            (!n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())).then(|| n.to_string())
        });

        // The NFS-e already exists at this point; a DANFSe failure must not force
        // re-issuing. The PDF stays unavailable and the XML is stored.
        let pdf = async {
            let d = ctx
                .client
                .get(format!("{}/danfse/{access_key}", urls.adn))
                .send()
                .await
                .ok()?;
            if d.status().as_u16() != 200 {
                return None;
            }
            d.bytes().await.ok().map(|b| b.to_vec())
        }
        .await;
        Ok(NfseResult {
            number,
            access_key: Some(access_key.to_string()),
            xml: nfse_xml,
            pdf,
            raw: Some(text),
        })
    }

    /// Walks the ADN DF-e distribution by NSU and keeps the NFS-e issued by the
    /// company (the feed also carries notes where it is the taker, and events).
    async fn list(&self) -> Result<Vec<ImportedNfse>> {
        let ctx = self.init()?;
        let urls = urls(&self.env.settings.nfse_env);
        let mut notes = Vec::new();
        let mut nsu: u64 = 0;
        loop {
            let res = ctx
                .client
                .get(format!(
                    "{}/contribuintes/DFe/{nsu}?cnpjConsulta={}&lote=true",
                    urls.adn, ctx.cfg.provider_cnpj
                ))
                .header("accept", "application/json")
                .send()
                .await
                .map_err(net)?;
            let status = res.status().as_u16();
            let text = res.text().await.unwrap_or_default();
            // 404 is how the portal says there is nothing after this NSU.
            if status == 404 {
                break;
            }
            if status >= 300 {
                let head: String = text.chars().take(300).collect();
                return Err(AppError::internal(format!("NFS-e {status}: {head}")));
            }
            let json: Value = serde_json::from_str(&text)?;
            let batch = json["LoteDFe"].as_array().cloned().unwrap_or_default();
            if batch.is_empty() {
                break;
            }
            let before = nsu;
            for doc in &batch {
                nsu = nsu.max(doc["NSU"].as_u64().unwrap_or(0));
                let (Some("NFSE"), Some(data)) =
                    (doc["TipoDocumento"].as_str(), doc["ArquivoXml"].as_str())
                else {
                    continue;
                };
                let parsed = parse_nfse(&gunzip_b64(data)?)?;
                if parsed.provider_tax_id.to_uppercase() == ctx.cfg.provider_cnpj {
                    notes.push(parsed.note);
                }
            }
            // A batch that does not advance would loop forever.
            if nsu == before {
                break;
            }
        }
        Ok(notes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gzip_round_trips_as_base64() {
        let text = "<DPS>Serviços</DPS>";
        assert_eq!(gunzip_b64(&gzip_b64(text).unwrap()).unwrap(), text);
    }
}
