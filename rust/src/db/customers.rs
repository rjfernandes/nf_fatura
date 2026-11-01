use crate::error::{AppError, Result};
use crate::integrations::types::BillingParty;
use crate::util::cnpj::{is_valid_cnpj, normalize_cnpj, only_digits};
use crate::util::{new_id, now_iso};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

/// Integrations a customer can be enabled for (Customer.integration).
pub const INTEGRATIONS: [&str; 1] = ["CAMIM"];

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Customer {
    pub id: String,
    pub name: String,
    pub cnpj: String,
    pub address: String,
    pub address_number: String,
    pub address_complement: Option<String>,
    pub neighborhood: String,
    pub city: String,
    pub city_ibge_code: Option<String>,
    pub state: String,
    pub zipcode: String,
    pub nick_name: Option<String>,
    pub station: Option<String>, // posto: one uppercase letter (A, Y, G...)
    pub recurring_value: i64,    // cents
    pub has_bank_slip: bool,
    pub integration: Option<String>, // delivery integration (CAMIM), None = none
    pub created_at: String,
    pub updated_at: String,
}

impl Customer {
    pub fn party(&self) -> BillingParty {
        BillingParty {
            name: self.name.clone(),
            cnpj: self.cnpj.clone(),
            address: self.address.clone(),
            address_number: self.address_number.clone(),
            address_complement: self.address_complement.clone(),
            neighborhood: self.neighborhood.clone(),
            city: self.city.clone(),
            city_ibge_code: self.city_ibge_code.clone(),
            state: self.state.clone(),
            zipcode: self.zipcode.clone(),
            station: self.station.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CustomerInput {
    pub name: String,
    pub cnpj: String,
    pub address: String,
    pub address_number: String,
    pub address_complement: Option<String>,
    pub neighborhood: String,
    pub city: String,
    pub city_ibge_code: Option<String>,
    pub state: String,
    pub zipcode: String,
    pub nick_name: Option<String>,
    pub station: Option<String>,
    pub recurring_value: i64,
    pub has_bank_slip: bool,
    pub integration: Option<String>,
}

impl CustomerInput {
    /// Same rules as the API; normalizes CNPJ, state, CEP and posto in place.
    pub fn validate(&mut self) -> Result<()> {
        let mut issues: Vec<String> = Vec::new();
        for (field, value) in [
            ("name", &self.name),
            ("address", &self.address),
            ("addressNumber", &self.address_number),
            ("neighborhood", &self.neighborhood),
            ("city", &self.city),
        ] {
            if value.is_empty() {
                issues.push(format!("{field} é obrigatório"));
            }
        }
        if is_valid_cnpj(&self.cnpj) {
            self.cnpj = normalize_cnpj(&self.cnpj);
        } else {
            issues.push("cnpj CNPJ inválido".into());
        }
        if let Some(code) = &self.city_ibge_code {
            if code.len() != 7 || !code.bytes().all(|b| b.is_ascii_digit()) {
                issues.push("cityIbgeCode Código IBGE deve ter 7 dígitos".into());
            }
        }
        if self.state.chars().count() == 2 {
            self.state = self.state.to_uppercase();
        } else {
            issues.push("state deve ter 2 letras".into());
        }
        self.zipcode = only_digits(&self.zipcode);
        if self.zipcode.len() != 8 {
            issues.push("zipcode CEP inválido".into());
        }
        self.nick_name = self.nick_name.take().map(|n| n.trim().to_string());
        if let Some(station) = self.station.take() {
            let station = station.trim().to_uppercase();
            if station.len() != 1 || !station.bytes().all(|b| b.is_ascii_uppercase()) {
                issues.push("station Posto deve ser uma única letra".into());
            }
            self.station = Some(station);
        }
        if self.recurring_value < 0 {
            issues.push("recurringValue não pode ser negativo".into());
        }
        if let Some(i) = &self.integration {
            if !INTEGRATIONS.contains(&i.as_str()) {
                issues.push("integration integração desconhecida".into());
            }
        }
        if issues.is_empty() {
            Ok(())
        } else {
            Err(AppError::new(
                400,
                format!("Dados inválidos: {}", issues.join("; ")),
            ))
        }
    }
}

const COLUMNS: &str = "id, name, cnpj, address, addressNumber, addressComplement, neighborhood, \
    city, cityIbgeCode, state, zipcode, nickName, station, recurringValue, hasBankSlip, \
    integration, createdAt, updatedAt";

fn from_row(r: &Row) -> rusqlite::Result<Customer> {
    Ok(Customer {
        id: r.get(0)?,
        name: r.get(1)?,
        cnpj: r.get(2)?,
        address: r.get(3)?,
        address_number: r.get(4)?,
        address_complement: r.get(5)?,
        neighborhood: r.get(6)?,
        city: r.get(7)?,
        city_ibge_code: r.get(8)?,
        state: r.get(9)?,
        zipcode: r.get(10)?,
        nick_name: r.get(11)?,
        station: r.get(12)?,
        recurring_value: r.get(13)?,
        has_bank_slip: r.get(14)?,
        integration: r.get(15)?,
        created_at: r.get(16)?,
        updated_at: r.get(17)?,
    })
}

pub fn list(c: &Connection) -> Result<Vec<Customer>> {
    let mut stmt = c.prepare(&format!("SELECT {COLUMNS} FROM Customer ORDER BY name ASC"))?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn find(c: &Connection, id: &str) -> Result<Option<Customer>> {
    Ok(c.query_row(
        &format!("SELECT {COLUMNS} FROM Customer WHERE id = ?1"),
        [id],
        from_row,
    )
    .optional()?)
}

fn conflict(e: rusqlite::Error) -> AppError {
    if super::is_unique_violation(&e) {
        AppError::new(409, "CNPJ já cadastrado")
    } else {
        e.into()
    }
}

/// Inserts an already validated customer.
pub fn create(c: &Connection, input: &CustomerInput) -> Result<Customer> {
    let id = new_id();
    let now = now_iso();
    c.execute(
        "INSERT INTO Customer (id, name, cnpj, address, addressNumber, addressComplement, \
         neighborhood, city, cityIbgeCode, state, zipcode, nickName, station, recurringValue, \
         hasBankSlip, integration, createdAt, updatedAt) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?17)",
        params![
            id,
            input.name,
            input.cnpj,
            input.address,
            input.address_number,
            input.address_complement,
            input.neighborhood,
            input.city,
            input.city_ibge_code,
            input.state,
            input.zipcode,
            input.nick_name,
            input.station,
            input.recurring_value,
            input.has_bank_slip,
            input.integration,
            now,
        ],
    )
    .map_err(conflict)?;
    find(c, &id)?.ok_or_else(|| AppError::internal("Cliente não encontrado após criar"))
}

pub fn update(c: &Connection, id: &str, input: &CustomerInput) -> Result<Customer> {
    let changed = c
        .execute(
            "UPDATE Customer SET name = ?2, cnpj = ?3, address = ?4, addressNumber = ?5, \
             addressComplement = ?6, neighborhood = ?7, city = ?8, cityIbgeCode = ?9, state = ?10, \
             zipcode = ?11, nickName = ?12, station = ?13, recurringValue = ?14, hasBankSlip = ?15, \
             integration = ?16, updatedAt = ?17 WHERE id = ?1",
            params![
                id,
                input.name,
                input.cnpj,
                input.address,
                input.address_number,
                input.address_complement,
                input.neighborhood,
                input.city,
                input.city_ibge_code,
                input.state,
                input.zipcode,
                input.nick_name,
                input.station,
                input.recurring_value,
                input.has_bank_slip,
                input.integration,
                now_iso(),
            ],
        )
        .map_err(conflict)?;
    if changed == 0 {
        return Err(AppError::new(404, "Não encontrado"));
    }
    find(c, id)?.ok_or_else(|| AppError::new(404, "Não encontrado"))
}

pub fn delete(c: &Connection, id: &str) -> Result<()> {
    let billings: i64 = c.query_row(
        "SELECT count(*) FROM Billing WHERE customerId = ?1",
        [id],
        |r| r.get(0),
    )?;
    if billings > 0 {
        return Err(AppError::new(409, "Cliente possui faturamentos"));
    }
    if c.execute("DELETE FROM Customer WHERE id = ?1", [id])? == 0 {
        return Err(AppError::new(404, "Não encontrado"));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn sample_input(cnpj: &str) -> CustomerInput {
    CustomerInput {
        name: "ACME LTDA".into(),
        cnpj: cnpj.into(),
        address: "Rua A".into(),
        address_number: "10".into(),
        neighborhood: "Centro".into(),
        city: "Niterói".into(),
        city_ibge_code: Some("3303302".into()),
        state: "rj".into(),
        zipcode: "24020-000".into(),
        recurring_value: 150000,
        ..CustomerInput::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn validates_and_normalizes_like_the_api() {
        let mut ok = sample_input("11.222.333/0001-81");
        ok.station = Some(" y ".into());
        ok.validate().unwrap();
        assert_eq!(ok.cnpj, "11222333000181");
        assert_eq!(ok.state, "RJ");
        assert_eq!(ok.zipcode, "24020000");
        assert_eq!(ok.station.as_deref(), Some("Y"));

        let mut bad = sample_input("11222333000182");
        bad.zipcode = "123".into();
        bad.station = Some("AB".into());
        let e = bad.validate().unwrap_err();
        assert_eq!(e.status, 400);
        assert!(e.message.contains("CNPJ inválido"));
        assert!(e.message.contains("CEP inválido"));
        assert!(e.message.contains("Posto deve ser uma única letra"));
    }

    #[test]
    fn creates_updates_and_deletes_with_the_api_errors() {
        let db = Db::in_memory().unwrap();
        db.with(|c| {
            let mut input = sample_input("11222333000181");
            input.validate()?;
            let created = create(c, &input)?;
            assert_eq!(list(c)?, vec![created.clone()]);
            assert_eq!(create(c, &input).unwrap_err().status, 409);

            input.name = "ACME S/A".into();
            assert_eq!(update(c, &created.id, &input)?.name, "ACME S/A");
            assert_eq!(update(c, "missing", &input).unwrap_err().status, 404);

            c.execute(
                "INSERT INTO Billing (id, customerId, competence, amountCents, dueDate, updatedAt) \
                 VALUES ('b1', ?1, '2026-09', 1, '2026-09-05', '')",
                [&created.id],
            )?;
            let e = delete(c, &created.id).unwrap_err();
            assert_eq!(
                (e.status, e.message.as_str()),
                (409, "Cliente possui faturamentos")
            );
            c.execute("DELETE FROM Billing", [])?;
            delete(c, &created.id)?;
            assert_eq!(delete(c, &created.id).unwrap_err().status, 404);
            Ok(())
        })
        .unwrap();
    }
}
