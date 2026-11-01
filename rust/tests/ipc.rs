//! The commands as the screens call them: argument names and shapes are the
//! ones web/src/desktop.ts sends.

use nf_fatura_lib::app::AppState;
use nf_fatura_lib::config::MemorySecrets;
use nf_fatura_lib::db::billings::{self, NewBilling};
use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{
    get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY,
};
use tauri::webview::InvokeRequest;
use tauri::{Manager, WebviewWindow, WebviewWindowBuilder};

struct Desktop {
    app: tauri::App<MockRuntime>,
    window: WebviewWindow<MockRuntime>,
}

impl Desktop {
    fn start() -> Self {
        let dir = std::env::temp_dir().join(format!("nf-fatura-ipc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let state = AppState::with_secrets(&dir, &dir, Box::<MemorySecrets>::default()).unwrap();
        let app = nf_fatura_lib::builder(mock_builder())
            .manage(state)
            .build(mock_context(noop_assets()))
            .unwrap();
        let window = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        Self { app, window }
    }

    fn request(
        &self,
        cmd: &str,
        body: InvokeBody,
        headers: &[(&'static str, &str)],
    ) -> Result<Value, Value> {
        let mut map = tauri::http::HeaderMap::new();
        for (name, value) in headers {
            map.insert(*name, value.parse().unwrap());
        }
        get_ipc_response(
            &self.window,
            InvokeRequest {
                cmd: cmd.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: "tauri://localhost".parse().unwrap(),
                body,
                headers: map,
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .map(|body| body.deserialize::<Value>().unwrap())
    }

    fn invoke(&self, cmd: &str, args: Value) -> Result<Value, Value> {
        self.request(cmd, InvokeBody::Json(args), &[])
    }
}

fn customer() -> Value {
    json!({
        "name": "ACME LTDA",
        "cnpj": "11.222.333/0001-81",
        "address": "Rua A",
        "addressNumber": "10",
        "addressComplement": null,
        "neighborhood": "Centro",
        "city": "Niterói",
        "cityIbgeCode": "3303302",
        "state": "rj",
        "zipcode": "24020-000",
        "nickName": "Acme",
        "station": null,
        "recurringValue": 150000,
        "hasBankSlip": true,
        "integration": null,
    })
}

#[test]
fn the_screens_reach_every_kind_of_command() {
    let d = Desktop::start();

    // Clientes
    let created = d
        .invoke("customers_create", json!({ "input": customer() }))
        .unwrap();
    assert_eq!(created["cnpj"], "11222333000181");
    assert_eq!(created["state"], "RJ");
    assert_eq!(created["recurringValue"], 150000);
    assert_eq!(created["hasBankSlip"], true);
    let id = created["id"].as_str().unwrap().to_string();
    let duplicate = d
        .invoke("customers_create", json!({ "input": customer() }))
        .unwrap_err();
    assert_eq!(
        duplicate,
        json!({ "status": 409, "message": "CNPJ já cadastrado" })
    );
    let mut invalid = customer();
    invalid["cnpj"] = "123".into();
    let e = d
        .invoke("customers_create", json!({ "input": invalid }))
        .unwrap_err();
    assert_eq!(e["status"], 400);
    assert!(e["message"].as_str().unwrap().contains("CNPJ inválido"));
    let mut edited = customer();
    edited["nickName"] = "Acme 2".into();
    let updated = d
        .invoke("customers_update", json!({ "id": id, "input": edited }))
        .unwrap();
    assert_eq!(updated["nickName"], "Acme 2");
    assert_eq!(
        d.invoke("customers_list", json!({}))
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // Faturamento
    assert_eq!(d.invoke("billings_list", json!({})).unwrap(), json!([]));
    let e = d
        .invoke(
            "billings_create",
            json!({ "input": { "customerIds": [], "competence": "2026-09", "dueDate": "2026-10-05", "mode": "AUTO" } }),
        )
        .unwrap_err();
    assert_eq!(e["status"], 400);
    let state = d.app.state::<AppState>();
    let billing = state
        .db
        .with(|c| {
            billings::create(
                c,
                &NewBilling {
                    customer_id: &id,
                    competence: "2026-09",
                    amount_cents: 150000,
                    due_date: "2026-10-05",
                    issue_nfse: false,
                    issue_slip: true,
                    status: "COMPLETED",
                },
            )
        })
        .unwrap();

    // The PDF travels as the raw body, the rest as (percent-encoded) headers.
    let attached = d
        .request(
            "nfse_attach",
            InvokeBody::Raw(b"%PDF-1.4 manual".to_vec()),
            &[
                ("x-billing-id", &billing),
                ("x-nfse-number", "N%C2%BA%2012"),
            ],
        )
        .unwrap();
    assert_eq!(attached["invoice"]["source"], "MANUAL");
    assert_eq!(attached["invoice"]["number"], "Nº 12");
    assert_eq!(attached["invoice"]["hasPdf"], true);
    assert_eq!(attached["customer"]["nickName"], "Acme 2");
    let not_pdf = d
        .request(
            "nfse_attach",
            InvokeBody::Raw(b"<html>".to_vec()),
            &[("x-billing-id", &billing)],
        )
        .unwrap_err();
    assert_eq!(
        not_pdf,
        json!({ "status": 400, "message": "Envie um arquivo PDF" })
    );

    let e = d
        .invoke("delivery_send", json!({ "id": billing, "parts": "NFSE" }))
        .unwrap_err();
    assert_eq!(
        e,
        json!({ "status": 409, "message": "Cliente sem integração de envio" })
    );
    let e = d
        .invoke("billing_delete", json!({ "id": billing }))
        .unwrap_err();
    assert_eq!(e["status"], 409);
    let e = d
        .invoke("slip_cancel", json!({ "id": billing }))
        .unwrap_err();
    assert_eq!(
        e,
        json!({ "status": 404, "message": "Boleto não encontrado" })
    );
    assert_eq!(
        d.invoke("nfse_remove", json!({ "id": billing })).unwrap(),
        Value::Null
    );
    let doc = json!({ "kind": "nfse_pdf", "id": billing, "fileName": "nota.pdf" });
    assert_eq!(
        d.invoke("document_open", doc.clone()).unwrap_err()["status"],
        404
    );
    assert_eq!(d.invoke("document_save", doc).unwrap_err()["status"], 404);
    let e = d
        .invoke("customers_delete", json!({ "id": id }))
        .unwrap_err();
    assert_eq!(e["message"], "Cliente possui faturamentos");
    let e = d
        .invoke("billings_import", json!({ "from": "outros" }))
        .unwrap_err();
    assert_eq!(e["status"], 400);

    // Extrato
    let e = d
        .invoke("statement_summary", json!({ "competence": "2999-01" }))
        .unwrap_err();
    assert_eq!(e["message"], "Escolha o mês atual ou um anterior");
    let e = d
        .invoke(
            "statement_export",
            json!({ "competence": "2026-09", "format": "csv" }),
        )
        .unwrap_err();
    assert_eq!(e["status"], 400);

    // Configurações
    let view = d.invoke("settings_get", json!({})).unwrap();
    assert_eq!(view["nfseEnv"], "homologacao");
    assert_eq!(view["interEnv"], "sandbox");
    assert_eq!(view["hasCamimToken"], false);
    assert!(view["databasePath"]
        .as_str()
        .unwrap()
        .ends_with("nf-fatura.db"));
    let integrations = d.invoke("integrations_list", json!({})).unwrap();
    assert_eq!(integrations[0]["configured"], false);
    assert_eq!(integrations[0]["requiredFields"], json!(["station"]));

    let mut settings = view.clone();
    for key in [
        "hasNfseCertPfxPassword",
        "hasInterClientSecret",
        "hasCamimToken",
        "databasePath",
    ] {
        settings.as_object_mut().unwrap().remove(key);
    }
    settings["companyCnpj"] = "03894412000100".into();
    settings["nfseDpsOffset"] = 14.into();
    let saved = d
        .invoke(
            "settings_save",
            json!({ "settings": settings, "secrets": { "camimToken": "t0ken" } }),
        )
        .unwrap();
    assert_eq!(saved["companyCnpj"], "03894412000100");
    assert_eq!(saved["nfseDpsOffset"], 14);
    assert_eq!(saved["hasCamimToken"], true);
    assert!(!saved.to_string().contains("t0ken"));
    // Saving rebuilds the services with the new settings.
    assert_eq!(
        d.invoke("integrations_list", json!({})).unwrap()[0]["configured"],
        true
    );
    settings["nfseEnv"] = "prod".into();
    let e = d
        .invoke(
            "settings_save",
            json!({ "settings": settings, "secrets": {} }),
        )
        .unwrap_err();
    assert_eq!(e["status"], 400);

    let e = d
        .invoke(
            "settings_import_env",
            json!({ "path": "/nonexistent/.env" }),
        )
        .unwrap_err();
    assert_eq!(e["status"], 400);
    let e = d
        .invoke("db_import", json!({ "path": "/nonexistent/x.db" }))
        .unwrap_err();
    assert_eq!(e["status"], 400);
}
