use std::time::Duration;

fn uf(code: &str) -> &'static str {
    match code.get(..2).unwrap_or("") {
        "11" => "RO",
        "12" => "AC",
        "13" => "AM",
        "14" => "RR",
        "15" => "PA",
        "16" => "AP",
        "17" => "TO",
        "21" => "MA",
        "22" => "PI",
        "23" => "CE",
        "24" => "RN",
        "25" => "PB",
        "26" => "PE",
        "27" => "AL",
        "28" => "SE",
        "29" => "BA",
        "31" => "MG",
        "32" => "ES",
        "33" => "RJ",
        "35" => "SP",
        "41" => "PR",
        "42" => "SC",
        "43" => "RS",
        "50" => "MS",
        "51" => "MT",
        "52" => "GO",
        "53" => "DF",
        _ => "-",
    }
}

/// City name and state for an IBGE municipality code. The state comes from the
/// code itself; the name from IBGE's public API, "-" when it can't be reached.
pub async fn city_of(code: &str) -> (String, String) {
    let state = uf(code).to_string();
    let name = async {
        let res = reqwest::Client::new()
            .get(format!(
                "https://servicodados.ibge.gov.br/api/v1/localidades/municipios/{code}"
            ))
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .ok()?;
        let data: serde_json::Value = res.json().await.ok()?;
        data.get("nome")?.as_str().map(str::to_string)
    }
    .await;
    (name.unwrap_or_else(|| "-".into()), state)
}
