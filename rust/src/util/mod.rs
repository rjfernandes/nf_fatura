pub mod cnpj;
pub mod ibge;
pub mod period;
pub mod statement_files;

/// Current instant in the format the database stores (ISO-8601, UTC, ms).
pub fn now_iso() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3f+00:00")
        .to_string()
}

pub fn new_id() -> String {
    cuid2::create_id()
}

/// pt-BR number with two decimals: 123456 -> "1.234,56" (sign left out).
pub fn brl_number(cents: i64) -> String {
    let abs = cents.unsigned_abs();
    let int = (abs / 100).to_string();
    let mut grouped = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i).is_multiple_of(3) {
            grouped.push('.');
        }
        grouped.push(ch);
    }
    format!("{grouped},{:02}", abs % 100)
}

/// Reais as a decimal string (e.g. "1234.5") to cents, rounded.
pub fn cents_of(value: f64) -> i64 {
    (value * 100.0).round() as i64
}
