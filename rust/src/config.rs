//! Settings of the app: what the server reads from its .env. Plain values
//! live in settings.json, secrets in the macOS Keychain, and certificates are
//! copied into the app's own folder.

use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const KEYCHAIN_SERVICE: &str = "br.com.mrf.nffatura";
const KEYCHAIN_ACCOUNT: &str = "secrets";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    // NFS-e Nacional
    pub nfse_env: String, // homologacao | producao
    pub nfse_cert_pfx_path: Option<String>,
    pub company_cnpj: Option<String>,
    pub company_name: String,
    pub company_municipal_registration: Option<String>,
    pub company_city_ibge: Option<String>,
    pub company_phone: Option<String>,
    pub company_email: Option<String>,
    pub nfse_service_code: String,           // cTribNac (6 digits)
    pub nfse_municipal_service_code: String, // cTribMun
    pub nfse_nbs_code: String,               // cNBS
    pub nfse_service_description: String,
    pub nfse_simples_nacional: String, // opSimpNac: 1 não optante, 2 MEI, 3 ME/EPP
    pub nfse_reg_ap_trib_sn: String,   // regApTribSN (only when opSimpNac=3)
    pub nfse_iss_rate_percent: f64,
    pub nfse_tot_trib_sn_percent: f64, // pTotTribSN (only when opSimpNac≠1)
    pub nfse_series: String,
    pub nfse_dps_offset: i64, // last nDPS already used in this series
    // Banco Inter
    pub inter_env: String, // sandbox | producao
    pub inter_client_id: Option<String>,
    pub inter_cert_path: Option<String>,
    pub inter_key_path: Option<String>,
    pub inter_account: Option<String>, // x-conta-corrente header
    pub inter_branch: String,          // shown on the exported statement
    // Camim (delivery of NFS-e + boleto)
    pub camim_base_url: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            nfse_env: "homologacao".into(),
            nfse_cert_pfx_path: None,
            company_cnpj: None,
            company_name: "MRF INFORMATICA LTDA".into(),
            company_municipal_registration: None,
            company_city_ibge: None,
            company_phone: None,
            company_email: None,
            nfse_service_code: "010701".into(),
            nfse_municipal_service_code: "001".into(),
            nfse_nbs_code: "115013000".into(),
            nfse_service_description:
                "Serviços de informática prestados Valor aproximado dos tributos: 6%".into(),
            nfse_simples_nacional: "3".into(),
            nfse_reg_ap_trib_sn: "1".into(),
            nfse_iss_rate_percent: 0.0,
            nfse_tot_trib_sn_percent: 6.0,
            nfse_series: "1".into(), // 1–49999: range of own applications (API)
            nfse_dps_offset: 0,
            inter_env: "sandbox".into(),
            inter_client_id: None,
            inter_cert_path: None,
            inter_key_path: None,
            inter_account: None,
            inter_branch: "0001-9".into(),
            camim_base_url: "https://pj.camim.com.br/api".into(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Secrets {
    pub nfse_cert_pfx_password: Option<String>,
    pub inter_client_secret: Option<String>,
    pub camim_token: Option<String>,
}

/// Everything the integrations need, as the server's `env`.
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub settings: Settings,
    pub secrets: Secrets,
}

fn blank_to_none(v: &mut Option<String>) {
    if v.as_deref().is_some_and(|s| s.trim().is_empty()) {
        *v = None;
    }
}

impl Settings {
    /// Same constraints the server's env schema enforces.
    pub fn validate(&mut self) -> Result<()> {
        for v in [
            &mut self.nfse_cert_pfx_path,
            &mut self.company_cnpj,
            &mut self.company_municipal_registration,
            &mut self.company_city_ibge,
            &mut self.company_phone,
            &mut self.company_email,
            &mut self.inter_client_id,
            &mut self.inter_cert_path,
            &mut self.inter_key_path,
            &mut self.inter_account,
        ] {
            blank_to_none(v);
        }
        let one_of = |name: &str, value: &str, allowed: &[&str]| {
            if allowed.contains(&value) {
                Ok(())
            } else {
                Err(AppError::new(
                    400,
                    format!("Dados inválidos: {name} deve ser {}", allowed.join(" ou ")),
                ))
            }
        };
        one_of(
            "ambiente da NFS-e",
            &self.nfse_env,
            &["homologacao", "producao"],
        )?;
        one_of(
            "ambiente do Banco Inter",
            &self.inter_env,
            &["sandbox", "producao"],
        )?;
        one_of(
            "Simples Nacional",
            &self.nfse_simples_nacional,
            &["1", "2", "3"],
        )?;
        one_of(
            "regime de apuração",
            &self.nfse_reg_ap_trib_sn,
            &["1", "2", "3"],
        )?;
        if self.nfse_dps_offset < 0 {
            return Err(AppError::new(
                400,
                "Dados inválidos: último nDPS não pode ser negativo",
            ));
        }
        Ok(())
    }
}

/// Where the secrets are kept; the Keychain in the app, memory in tests.
pub trait SecretStore: Send + Sync {
    fn load(&self) -> Result<Secrets>;
    fn save(&self, secrets: &Secrets) -> Result<()>;
}

/// One Keychain item holding all secrets, so macOS asks for access once.
pub struct Keychain;

impl Keychain {
    fn entry() -> Result<keyring::Entry> {
        keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
            .map_err(|e| AppError::internal(format!("Keychain: {e}")))
    }
}

impl SecretStore for Keychain {
    fn load(&self) -> Result<Secrets> {
        match Self::entry()?.get_password() {
            Ok(json) => Ok(serde_json::from_str(&json).unwrap_or_default()),
            Err(keyring::Error::NoEntry) => Ok(Secrets::default()),
            Err(e) => Err(AppError::internal(format!("Keychain: {e}"))),
        }
    }

    fn save(&self, secrets: &Secrets) -> Result<()> {
        Self::entry()?
            .set_password(&serde_json::to_string(secrets)?)
            .map_err(|e| AppError::internal(format!("Keychain: {e}")))
    }
}

#[derive(Default)]
pub struct MemorySecrets(Mutex<Secrets>);

impl SecretStore for MemorySecrets {
    fn load(&self) -> Result<Secrets> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn save(&self, secrets: &Secrets) -> Result<()> {
        *self.0.lock().unwrap() = secrets.clone();
        Ok(())
    }
}

/// New values for the secrets: None keeps the stored one, "" removes it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SecretsUpdate {
    pub nfse_cert_pfx_password: Option<String>,
    pub inter_client_secret: Option<String>,
    pub camim_token: Option<String>,
}

/// What the settings screen sees: the secrets only as "is it stored?".
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    #[serde(flatten)]
    pub settings: Settings,
    pub has_nfse_cert_pfx_password: bool,
    pub has_inter_client_secret: bool,
    pub has_camim_token: bool,
    pub database_path: String,
}

pub struct ConfigStore {
    dir: PathBuf,
    secret_store: Box<dyn SecretStore>,
    // Read once: every Keychain read of an unsigned build may prompt the user.
    secrets: Mutex<Option<Secrets>>,
}

impl ConfigStore {
    pub fn new(dir: PathBuf, secret_store: Box<dyn SecretStore>) -> Self {
        Self {
            dir,
            secret_store,
            secrets: Mutex::new(None),
        }
    }

    fn file(&self) -> PathBuf {
        self.dir.join("settings.json")
    }

    pub fn settings(&self) -> Result<Settings> {
        match std::fs::read_to_string(self.file()) {
            Ok(json) => Ok(serde_json::from_str(&json)
                .map_err(|e| AppError::internal(format!("settings.json inválido: {e}")))?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn secrets(&self) -> Result<Secrets> {
        let mut cached = self.secrets.lock().unwrap();
        if cached.is_none() {
            *cached = Some(self.secret_store.load()?);
        }
        Ok(cached.clone().unwrap_or_default())
    }

    pub fn env(&self) -> Result<Env> {
        Ok(Env {
            settings: self.settings()?,
            secrets: self.secrets()?,
        })
    }

    /// A certificate picked outside the app's folder is copied into it, only
    /// readable by the user.
    fn adopt_certificate(&self, path: &mut Option<String>) -> Result<()> {
        let Some(current) = path.as_deref() else {
            return Ok(());
        };
        let certs = self.dir.join("certs");
        let source = Path::new(current);
        if source.starts_with(&certs) {
            return Ok(());
        }
        let name = source
            .file_name()
            .ok_or_else(|| AppError::new(400, format!("Arquivo inválido: {current}")))?;
        std::fs::create_dir_all(&certs)?;
        let target = certs.join(name);
        std::fs::copy(source, &target)
            .map_err(|e| AppError::new(400, format!("Não foi possível ler {current}: {e}")))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600))?;
        }
        *path = Some(target.to_string_lossy().into_owned());
        Ok(())
    }

    pub fn save(&self, mut settings: Settings, update: SecretsUpdate) -> Result<()> {
        settings.validate()?;
        self.adopt_certificate(&mut settings.nfse_cert_pfx_path)?;
        self.adopt_certificate(&mut settings.inter_cert_path)?;
        self.adopt_certificate(&mut settings.inter_key_path)?;

        let mut secrets = self.secrets()?;
        let before = secrets.clone();
        let apply = |target: &mut Option<String>, new: Option<String>| {
            if let Some(v) = new {
                *target = if v.is_empty() { None } else { Some(v) };
            }
        };
        apply(
            &mut secrets.nfse_cert_pfx_password,
            update.nfse_cert_pfx_password,
        );
        apply(&mut secrets.inter_client_secret, update.inter_client_secret);
        apply(&mut secrets.camim_token, update.camim_token);
        if secrets != before {
            self.secret_store.save(&secrets)?;
            *self.secrets.lock().unwrap() = Some(secrets);
        }

        std::fs::create_dir_all(&self.dir)?;
        std::fs::write(self.file(), serde_json::to_string_pretty(&settings)?)?;
        Ok(())
    }

    /// Reads the server's .env: values, secrets and the certificates it points
    /// to (paths relative to the .env's folder).
    pub fn import_env(&self, env_file: &Path) -> Result<()> {
        let text = std::fs::read_to_string(env_file)
            .map_err(|e| AppError::new(400, format!("Não foi possível ler o .env: {e}")))?;
        let vars = parse_env(&text);
        let base = env_file.parent().unwrap_or(Path::new("."));
        let get = |k: &str| vars.get(k).filter(|v| !v.is_empty()).cloned();
        let path = |k: &str| {
            get(k).map(|v| {
                let p = Path::new(&v);
                if p.is_absolute() {
                    v
                } else {
                    base.join(p).to_string_lossy().into_owned()
                }
            })
        };
        let mut s = self.settings()?;
        let set = |target: &mut String, k: &str| {
            if let Some(v) = get(k) {
                *target = v;
            }
        };
        let set_opt = |target: &mut Option<String>, v: Option<String>| {
            if v.is_some() {
                *target = v;
            }
        };
        let number = |k: &str| -> Result<Option<f64>> {
            get(k)
                .map(|v| {
                    v.parse::<f64>()
                        .map_err(|_| AppError::new(400, format!(".env: {k} deve ser um número")))
                })
                .transpose()
        };
        set(&mut s.nfse_env, "NFSE_ENV");
        set_opt(&mut s.nfse_cert_pfx_path, path("NFSE_CERT_PFX_PATH"));
        set_opt(&mut s.company_cnpj, get("COMPANY_CNPJ"));
        set(&mut s.company_name, "COMPANY_NAME");
        set_opt(
            &mut s.company_municipal_registration,
            get("COMPANY_MUNICIPAL_REGISTRATION"),
        );
        set_opt(&mut s.company_city_ibge, get("COMPANY_CITY_IBGE"));
        set_opt(&mut s.company_phone, get("COMPANY_PHONE"));
        set_opt(&mut s.company_email, get("COMPANY_EMAIL"));
        set(&mut s.nfse_service_code, "NFSE_SERVICE_CODE");
        set(
            &mut s.nfse_municipal_service_code,
            "NFSE_MUNICIPAL_SERVICE_CODE",
        );
        set(&mut s.nfse_nbs_code, "NFSE_NBS_CODE");
        set(&mut s.nfse_service_description, "NFSE_SERVICE_DESCRIPTION");
        set(&mut s.nfse_simples_nacional, "NFSE_SIMPLES_NACIONAL");
        set(&mut s.nfse_reg_ap_trib_sn, "NFSE_REG_AP_TRIB_SN");
        if let Some(v) = number("NFSE_ISS_RATE_PERCENT")? {
            s.nfse_iss_rate_percent = v;
        }
        if let Some(v) = number("NFSE_TOT_TRIB_SN_PERCENT")? {
            s.nfse_tot_trib_sn_percent = v;
        }
        set(&mut s.nfse_series, "NFSE_SERIES");
        if let Some(v) = number("NFSE_DPS_OFFSET")? {
            s.nfse_dps_offset = v as i64;
        }
        set(&mut s.inter_env, "INTER_ENV");
        set_opt(&mut s.inter_client_id, get("INTER_CLIENT_ID"));
        set_opt(&mut s.inter_cert_path, path("INTER_CERT_PATH"));
        set_opt(&mut s.inter_key_path, path("INTER_KEY_PATH"));
        set_opt(&mut s.inter_account, get("INTER_ACCOUNT"));
        set(&mut s.inter_branch, "INTER_BRANCH");
        set(&mut s.camim_base_url, "CAMIM_BASE_URL");
        self.save(
            s,
            SecretsUpdate {
                nfse_cert_pfx_password: get("NFSE_CERT_PFX_PASSWORD"),
                inter_client_secret: get("INTER_CLIENT_SECRET"),
                camim_token: get("CAMIM_TOKEN"),
            },
        )
    }

    pub fn view(&self, database_path: &Path) -> Result<SettingsView> {
        let secrets = self.secrets()?;
        Ok(SettingsView {
            settings: self.settings()?,
            has_nfse_cert_pfx_password: secrets.nfse_cert_pfx_password.is_some(),
            has_inter_client_secret: secrets.inter_client_secret.is_some(),
            has_camim_token: secrets.camim_token.is_some(),
            database_path: database_path.to_string_lossy().into_owned(),
        })
    }
}

/// KEY=value lines as dotenv reads them: quotes are stripped, and an unquoted
/// value ends at a " #" comment.
pub fn parse_env(text: &str) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        let value = match value.chars().next() {
            Some(q @ ('"' | '\'')) => match value[1..].find(q) {
                Some(end) => &value[1..=end],
                None => value,
            },
            _ => {
                let end = value
                    .char_indices()
                    .find(|(i, c)| {
                        *c == '#' && (*i == 0 || value[..*i].ends_with(char::is_whitespace))
                    })
                    .map_or(value.len(), |(i, _)| i);
                value[..end].trim()
            }
        };
        vars.insert(key.trim().to_string(), value.to_string());
    }
    vars
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nf-fatura-{name}-{}", crate::util::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parses_env_lines_like_dotenv() {
        let vars = parse_env(
            "# comment\nPORT=3333\nNFSE_ENV=homologacao            # homologacao | producao\n\
             COMPANY_NAME=\"MRF INFORMATICA LTDA\"  # cabeçalho\nEMPTY=\nURL=https://a/b#c\n",
        );
        assert_eq!(vars["PORT"], "3333");
        assert_eq!(vars["NFSE_ENV"], "homologacao");
        assert_eq!(vars["COMPANY_NAME"], "MRF INFORMATICA LTDA");
        assert_eq!(vars["EMPTY"], "");
        assert_eq!(vars["URL"], "https://a/b#c");
    }

    #[test]
    fn starts_in_the_test_environments() {
        let store = ConfigStore::new(temp_dir("defaults"), Box::<MemorySecrets>::default());
        let s = store.settings().unwrap();
        assert_eq!(s.nfse_env, "homologacao");
        assert_eq!(s.inter_env, "sandbox");
    }

    #[test]
    fn imports_the_server_env_with_secrets_and_certificates() {
        let server = temp_dir("server");
        std::fs::create_dir_all(server.join("certs")).unwrap();
        std::fs::write(server.join("certs/inter.crt"), "CERT").unwrap();
        std::fs::write(
            server.join(".env"),
            "INTER_ENV=producao\nINTER_CLIENT_ID=abc\nINTER_CLIENT_SECRET=s3cret\n\
             INTER_CERT_PATH=./certs/inter.crt\nNFSE_DPS_OFFSET=14 # último\nCAMIM_TOKEN=\n",
        )
        .unwrap();
        let app = temp_dir("app");
        let store = ConfigStore::new(app.clone(), Box::<MemorySecrets>::default());
        store.import_env(&server.join(".env")).unwrap();

        let env = store.env().unwrap();
        assert_eq!(env.settings.inter_env, "producao");
        assert_eq!(env.settings.inter_client_id.as_deref(), Some("abc"));
        assert_eq!(env.settings.nfse_dps_offset, 14);
        assert_eq!(env.secrets.inter_client_secret.as_deref(), Some("s3cret"));
        assert_eq!(env.secrets.camim_token, None);
        let cert = env.settings.inter_cert_path.unwrap();
        assert!(cert.starts_with(app.join("certs").to_str().unwrap()));
        assert_eq!(std::fs::read_to_string(cert).unwrap(), "CERT");
        // Secrets never reach settings.json.
        assert!(!std::fs::read_to_string(app.join("settings.json"))
            .unwrap()
            .contains("s3cret"));
    }

    #[test]
    fn keeps_or_removes_a_stored_secret() {
        let store = ConfigStore::new(temp_dir("secrets"), Box::<MemorySecrets>::default());
        let update = |token: Option<&str>| SecretsUpdate {
            camim_token: token.map(str::to_string),
            ..SecretsUpdate::default()
        };
        store
            .save(Settings::default(), update(Some("t0ken")))
            .unwrap();
        store.save(Settings::default(), update(None)).unwrap();
        assert_eq!(
            store.secrets().unwrap().camim_token.as_deref(),
            Some("t0ken")
        );
        store.save(Settings::default(), update(Some(""))).unwrap();
        assert_eq!(store.secrets().unwrap().camim_token, None);
    }

    #[test]
    fn rejects_unknown_environments() {
        let mut s = Settings {
            nfse_env: "prod".into(),
            ..Settings::default()
        };
        assert_eq!(s.validate().unwrap_err().status, 400);
    }
}
