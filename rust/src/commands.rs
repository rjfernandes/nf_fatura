//! One command per operation of the web API (web/src/api.ts), plus what only
//! the desktop has: native dialogs, files and the settings.

use crate::app::AppState;
use crate::config::{SecretsUpdate, Settings, SettingsView};
use crate::db::billings::{self, BillingView};
use crate::db::customers::{self, Customer, CustomerInput};
use crate::error::{http_error, AppError, Result};
use crate::integrations::types::DeliveryParts;
use crate::services::billing::{BillingInput, SlipCancelled, MAX_PDF_BYTES};
use crate::services::delivery::Integration;
use crate::services::import::ImportResult;
use crate::services::statement::StatementSummary;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_opener::OpenerExt;

type S<'a> = State<'a, AppState>;

// ------------------------------------------------------------------ customers

#[tauri::command]
pub async fn customers_list(state: S<'_>) -> Result<Vec<Customer>> {
    state.db.with(|c| customers::list(c))
}

#[tauri::command]
pub async fn customers_create(state: S<'_>, mut input: CustomerInput) -> Result<Customer> {
    input.validate()?;
    state.db.with(|c| customers::create(c, &input))
}

#[tauri::command]
pub async fn customers_update(
    state: S<'_>,
    id: String,
    mut input: CustomerInput,
) -> Result<Customer> {
    input.validate()?;
    state.db.with(|c| customers::update(c, &id, &input))
}

#[tauri::command]
pub async fn customers_delete(state: S<'_>, id: String) -> Result<()> {
    state.db.with(|c| customers::delete(c, &id))
}

// ------------------------------------------------------------------- billings

#[tauri::command]
pub async fn billings_list(state: S<'_>) -> Result<Vec<BillingView>> {
    state.db.with(|c| billings::list(c))
}

#[tauri::command]
pub async fn billings_create(state: S<'_>, input: BillingInput) -> Result<Vec<BillingView>> {
    input.validate()?;
    state.services()?.billing.run(&input).await
}

/// Imports the NFS-e already issued by the company from the national portal
/// ("nfse") or the boletos registered at Banco Inter ("slips").
#[tauri::command]
pub async fn billings_import(state: S<'_>, from: String) -> Result<ImportResult> {
    let services = state.services()?;
    match from.as_str() {
        "nfse" => services.importer.import_from_nfse().await,
        "slips" => services.importer.import_from_slips().await,
        _ => Err(http_error(400, "Dados inválidos: origem desconhecida")),
    }
}

#[tauri::command]
pub async fn billing_retry(state: S<'_>, id: String) -> Result<BillingView> {
    state.services()?.billing.process(&id).await?;
    state.db.with(|c| billings::view(c, &id))
}

#[tauri::command]
pub async fn billing_delete(state: S<'_>, id: String) -> Result<()> {
    state.services()?.billing.remove(&id)
}

fn header(request: &tauri::ipc::Request<'_>, name: &str) -> Option<String> {
    let value = request.headers().get(name)?.to_str().ok()?;
    percent_encoding::percent_decode_str(value)
        .decode_utf8()
        .ok()
        .map(|v| v.into_owned())
}

/// Attaches the PDF of an NFS-e issued outside the system. The body is the raw
/// PDF; the billing and the optional number come as headers.
#[tauri::command]
pub async fn nfse_attach(request: tauri::ipc::Request<'_>, state: S<'_>) -> Result<BillingView> {
    let tauri::ipc::InvokeBody::Raw(pdf) = request.body() else {
        return Err(http_error(400, "Envie um arquivo PDF"));
    };
    if pdf.len() > MAX_PDF_BYTES {
        return Err(http_error(413, "O PDF deve ter no máximo 10 MB."));
    }
    let id = header(&request, "x-billing-id")
        .ok_or_else(|| http_error(400, "Dados inválidos: faturamento não informado"))?;
    let number = header(&request, "x-nfse-number");
    state
        .services()?
        .billing
        .attach_nfse_pdf(&id, pdf, number.as_deref())?;
    state.db.with(|c| billings::view(c, &id))
}

#[tauri::command]
pub async fn nfse_remove(state: S<'_>, id: String) -> Result<()> {
    state.services()?.billing.remove_manual_nfse(&id)
}

/// Cancels the boleto at Banco Inter; the billing is removed too unless it has
/// an NFS-e.
#[tauri::command]
pub async fn slip_cancel(state: S<'_>, id: String) -> Result<SlipCancelled> {
    state.services()?.billing.cancel_slip(&id).await
}

/// Sends the NFS-e, the boleto or both through the customer's integration.
#[tauri::command]
pub async fn delivery_send(state: S<'_>, id: String, parts: Option<String>) -> Result<BillingView> {
    let parts = DeliveryParts::parse(parts.as_deref().unwrap_or("BOTH"))
        .ok_or_else(|| http_error(400, "Dados inválidos: parts deve ser NFSE, SLIP ou BOTH"))?;
    state.services()?.delivery.send(&id, parts).await?;
    state.db.with(|c| billings::view(c, &id))
}

#[tauri::command]
pub async fn integrations_list(state: S<'_>) -> Result<Vec<Integration>> {
    Ok(state.services()?.delivery.list())
}

// ------------------------------------------------------------------ documents

/// A file name without anything that could leave the folder it is written to.
fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || "._- ".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches(|c| c == '.' || c == ' ').to_string();
    if cleaned.is_empty() {
        "documento".into()
    } else {
        cleaned
    }
}

fn document(state: &AppState, kind: &str, id: &str) -> Result<Vec<u8>> {
    let billing = state.services()?.billing.clone();
    match kind {
        "nfse_pdf" => billing
            .nfse_pdf(id)?
            .ok_or_else(|| http_error(404, "PDF não disponível")),
        "nfse_xml" => billing
            .nfse_xml(id)?
            .map(String::into_bytes)
            .ok_or_else(|| http_error(404, "NFS-e não encontrada")),
        "slip_pdf" => billing
            .slip_pdf(id)?
            .ok_or_else(|| http_error(404, "Boleto não encontrado")),
        _ => Err(http_error(400, "Dados inválidos: documento desconhecido")),
    }
}

/// Asks where to save and writes the file. False when the user cancels.
fn save_as<R: Runtime>(app: &AppHandle<R>, file_name: &str, content: &[u8]) -> Result<bool> {
    let name = safe_name(file_name);
    let mut dialog = app.dialog().file().set_file_name(&name);
    if let Some(ext) = Path::new(&name).extension().and_then(|e| e.to_str()) {
        dialog = dialog.add_filter(ext.to_uppercase(), &[ext]);
    }
    let Some(target) = dialog.blocking_save_file() else {
        return Ok(false);
    };
    let path: PathBuf = target
        .into_path()
        .map_err(|e| AppError::internal(format!("Local inválido: {e}")))?;
    std::fs::write(&path, content)
        .map_err(|e| AppError::internal(format!("Não foi possível salvar o arquivo: {e}")))?;
    Ok(true)
}

/// Opens the NFS-e or the boleto in the system's viewer (Preview).
#[tauri::command]
pub async fn document_open<R: Runtime>(
    app: AppHandle<R>,
    state: S<'_>,
    kind: String,
    id: String,
    file_name: String,
) -> Result<()> {
    let content = document(&state, &kind, &id)?;
    let dir = state.cache_dir.join("documentos");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(safe_name(&file_name));
    std::fs::write(&path, content)?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| AppError::internal(format!("Não foi possível abrir o arquivo: {e}")))
}

#[tauri::command]
pub async fn document_save<R: Runtime>(
    app: AppHandle<R>,
    state: S<'_>,
    kind: String,
    id: String,
    file_name: String,
) -> Result<bool> {
    let content = document(&state, &kind, &id)?;
    save_as(&app, &file_name, &content)
}

/// Saves a text built by the screen (the statement CSV).
#[tauri::command]
pub async fn text_save<R: Runtime>(
    app: AppHandle<R>,
    file_name: String,
    content: String,
) -> Result<bool> {
    save_as(&app, &file_name, content.as_bytes())
}

// ------------------------------------------------------------------ statement

#[tauri::command]
pub async fn statement_summary(state: S<'_>, competence: String) -> Result<StatementSummary> {
    state.services()?.statements.summary(&competence).await
}

#[tauri::command]
pub async fn statement_export<R: Runtime>(
    app: AppHandle<R>,
    state: S<'_>,
    competence: String,
    format: String,
) -> Result<bool> {
    let file = state
        .services()?
        .statements
        .export(&competence, &format)
        .await?;
    save_as(&app, &file.file_name, &file.content)
}

// ------------------------------------------------------------------- settings

fn settings_view(state: &AppState) -> Result<SettingsView> {
    state.config.view(state.db.path())
}

#[tauri::command]
pub async fn settings_get(state: S<'_>) -> Result<SettingsView> {
    settings_view(&state)
}

#[tauri::command]
pub async fn settings_save(
    state: S<'_>,
    settings: Settings,
    secrets: Option<SecretsUpdate>,
) -> Result<SettingsView> {
    state.config.save(settings, secrets.unwrap_or_default())?;
    state.reset_services();
    settings_view(&state)
}

/// Brings in the values, secrets and certificates of the server's .env.
#[tauri::command]
pub async fn settings_import_env(state: S<'_>, path: String) -> Result<SettingsView> {
    state.config.import_env(Path::new(&path))?;
    state.reset_services();
    settings_view(&state)
}

#[derive(Serialize)]
pub struct DbImported {
    /// Where the database that was replaced has been kept.
    backup: Option<String>,
}

/// Replaces the app's database with a copy of the server's.
#[tauri::command]
pub async fn db_import(state: S<'_>, path: String) -> Result<DbImported> {
    let backup = state.db.import(Path::new(&path))?;
    Ok(DbImported {
        backup: backup.map(|p| p.to_string_lossy().into_owned()),
    })
}

/// Lets the user pick a file; None when cancelled.
#[tauri::command]
pub async fn pick_file<R: Runtime>(
    app: AppHandle<R>,
    title: Option<String>,
    extensions: Option<Vec<String>>,
) -> Result<Option<String>> {
    let mut dialog = app.dialog().file();
    if let Some(title) = title {
        dialog = dialog.set_title(title);
    }
    if let Some(extensions) = extensions.filter(|e| !e.is_empty()) {
        let extensions: Vec<&str> = extensions.iter().map(String::as_str).collect();
        dialog = dialog.add_filter("Arquivos", &extensions);
    }
    Ok(dialog
        .blocking_pick_file()
        .and_then(|f| f.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned()))
}

// -------------------------------------------------------------------- dialogs

// The WebView has no window.confirm/alert of its own.

#[tauri::command]
pub async fn dialog_confirm<R: Runtime>(app: AppHandle<R>, message: String) -> Result<bool> {
    Ok(app
        .dialog()
        .message(message)
        .title("NF Fatura")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Confirmar".into(),
            "Cancelar".into(),
        ))
        .blocking_show())
}

#[tauri::command]
pub async fn dialog_alert<R: Runtime>(app: AppHandle<R>, message: String) -> Result<()> {
    app.dialog()
        .message(message)
        .title("NF Fatura")
        .kind(MessageDialogKind::Error)
        .blocking_show();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::safe_name;

    #[test]
    fn file_names_cannot_leave_their_folder() {
        assert_eq!(
            safe_name("nota_fiscal_2026-09_acme.pdf"),
            "nota_fiscal_2026-09_acme.pdf"
        );
        assert_eq!(safe_name("../../etc/passwd"), "_.._etc_passwd");
        assert_eq!(safe_name(".."), "documento");
    }
}
