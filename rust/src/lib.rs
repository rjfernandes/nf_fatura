pub mod app;
mod commands;
pub mod config;
pub mod db;
pub mod error;
pub mod integrations;
pub mod pdf;
pub mod services;
#[cfg(test)]
mod testing;
pub mod util;

use tauri::Manager;

/// Plugins and commands of the app, on any runtime (the tests use a mock one).
pub fn builder<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::customers_list,
            commands::customers_create,
            commands::customers_update,
            commands::customers_delete,
            commands::billings_list,
            commands::billings_create,
            commands::billings_import,
            commands::billing_retry,
            commands::billing_delete,
            commands::nfse_attach,
            commands::nfse_remove,
            commands::slip_cancel,
            commands::delivery_send,
            commands::integrations_list,
            commands::document_open,
            commands::document_save,
            commands::text_save,
            commands::statement_summary,
            commands::statement_export,
            commands::settings_get,
            commands::settings_save,
            commands::settings_import_env,
            commands::db_import,
            commands::pick_file,
            commands::dialog_confirm,
            commands::dialog_alert,
        ])
}

pub fn run() {
    builder(tauri::Builder::default())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let cache_dir = app.path().app_cache_dir()?;
            app.manage(app::AppState::open(&data_dir, &cache_dir)?);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o NF Fatura");
}
