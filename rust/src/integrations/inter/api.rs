use crate::config::Env;
use crate::error::{AppError, Result};
use reqwest::Method;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

fn base(env: &str) -> &'static str {
    if env == "producao" {
        "https://cdpj.partners.bancointer.com.br"
    } else {
        "https://cdpj-sandbox.partners.uatinter.co"
    }
}

/// Shared mTLS + OAuth2 plumbing for every Banco Inter API (boleto, extrato...).
pub struct InterApi {
    env: Env,
    client: Mutex<Option<reqwest::Client>>,
    // One token per scope set, so a missing scope on the integration only breaks
    // the feature that needs it.
    tokens: Mutex<HashMap<String, (String, Instant)>>,
}

impl InterApi {
    pub fn new(env: Env) -> Self {
        Self {
            env,
            client: Mutex::new(None),
            tokens: Mutex::new(HashMap::new()),
        }
    }

    fn base(&self) -> &'static str {
        base(&self.env.settings.inter_env)
    }

    fn client(&self) -> Result<reqwest::Client> {
        let mut cached = self.client.lock().unwrap();
        if let Some(client) = cached.as_ref() {
            return Ok(client.clone());
        }
        let s = &self.env.settings;
        let (Some(cert), Some(key)) = (&s.inter_cert_path, &s.inter_key_path) else {
            return Err(AppError::internal(
                "Banco Inter: certificado e chave não configurados",
            ));
        };
        let read = |path: &str| {
            std::fs::read(path).map_err(|e| {
                AppError::internal(format!("Banco Inter: não foi possível ler {path}: {e}"))
            })
        };
        let mut pem = read(cert)?;
        pem.push(b'\n');
        pem.extend(read(key)?);
        let identity = reqwest::Identity::from_pem(&pem)
            .map_err(|e| AppError::internal(format!("Banco Inter: certificado inválido: {e}")))?;
        let client = reqwest::Client::builder()
            .use_rustls_tls()
            .identity(identity)
            .timeout(Duration::from_secs(60))
            .build()?;
        *cached = Some(client.clone());
        Ok(client)
    }

    async fn token(&self, scope: &str) -> Result<String> {
        if let Some((value, expires_at)) = self.tokens.lock().unwrap().get(scope) {
            if *expires_at > Instant::now() + Duration::from_secs(30) {
                return Ok(value.clone());
            }
        }
        let (Some(id), Some(secret)) = (
            &self.env.settings.inter_client_id,
            &self.env.secrets.inter_client_secret,
        ) else {
            return Err(AppError::internal(
                "Banco Inter: client id e client secret não configurados",
            ));
        };
        let res = self
            .client()?
            .post(format!("{}/oauth/v2/token", self.base()))
            .form(&[
                ("client_id", id.as_str()),
                ("client_secret", secret.as_str()),
                ("grant_type", "client_credentials"),
                ("scope", scope),
            ])
            .send()
            .await
            .map_err(|e| AppError::internal(format!("Banco Inter auth: {}", describe(&e))))?;
        let status = res.status().as_u16();
        let text = res.text().await.unwrap_or_default();
        if status != 200 {
            return Err(AppError::internal(format!(
                "Banco Inter auth {status}: {text}"
            )));
        }
        let json: Value = serde_json::from_str(&text)?;
        let value = json["access_token"]
            .as_str()
            .ok_or_else(|| {
                AppError::internal(format!("Banco Inter auth: resposta sem token: {text}"))
            })?
            .to_string();
        let expires_in = json["expires_in"].as_u64().unwrap_or(0);
        self.tokens.lock().unwrap().insert(
            scope.to_string(),
            (
                value.clone(),
                Instant::now() + Duration::from_secs(expires_in),
            ),
        );
        Ok(value)
    }

    /// Calls the API and returns the JSON answer ({} when there is no body).
    pub async fn call(
        &self,
        scope: &str,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value> {
        let token = self.token(scope).await?;
        let mut req = self
            .client()?
            .request(method.clone(), format!("{}{path}", self.base()))
            .bearer_auth(token)
            .header("content-type", "application/json");
        if let Some(account) = &self.env.settings.inter_account {
            req = req.header("x-conta-corrente", account);
        }
        if let Some(body) = body {
            req = req.body(body.to_string());
        }
        let res = req.send().await.map_err(|e| {
            AppError::internal(format!("Banco Inter {method} {path}: {}", describe(&e)))
        })?;
        let status = res.status().as_u16();
        let text = res.text().await.unwrap_or_default();
        if status >= 300 {
            return Err(AppError::internal(format!(
                "Banco Inter {method} {path} {status}: {text}"
            )));
        }
        if text.is_empty() {
            Ok(Value::Object(Default::default()))
        } else {
            Ok(serde_json::from_str(&text)?)
        }
    }
}

/// reqwest hides the cause (TLS, DNS...) behind "error sending request".
pub fn describe(e: &reqwest::Error) -> String {
    let mut out = e.to_string();
    let mut source = std::error::Error::source(e);
    while let Some(s) = source {
        out.push_str(": ");
        out.push_str(&s.to_string());
        source = s.source();
    }
    out
}

/// A JSON field as text, whether the bank sent a string or a number.
pub fn text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// A JSON amount in reais (string or number) as cents.
pub fn cents(v: &Value) -> i64 {
    let n = match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    };
    crate::util::cents_of(n)
}
