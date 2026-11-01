//! SQLite with the same schema and value formats Prisma writes, so a database
//! of the server can be brought in as it is.

pub mod billings;
pub mod customers;

use crate::error::{AppError, Result};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const SCHEMA: &str = include_str!("schema.sql");
const TABLES: [&str; 6] = [
    "Customer",
    "Billing",
    "Invoice",
    "BankSlip",
    "Delivery",
    "DpsSequence",
];

pub struct Db {
    conn: Mutex<Connection>,
    path: Option<PathBuf>,
}

fn prepare(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(SCHEMA)?;
    // Prisma declares it; a database created before that may hold duplicates,
    // which must not keep the app from opening.
    let _ = conn.execute_batch(
        r#"CREATE UNIQUE INDEX IF NOT EXISTS "Invoice_accessKey_key" ON "Invoice"("accessKey");"#,
    );
    Ok(())
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        prepare(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            path: Some(path.to_path_buf()),
        })
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        prepare(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            path: None,
        })
    }

    pub fn path(&self) -> &Path {
        self.path.as_deref().unwrap_or(Path::new(":memory:"))
    }

    /// Runs `f` with the connection. Never held across an await: load, call
    /// the network, then come back to write.
    pub fn with<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut conn)
    }

    /// Replaces the app's database with a copy of another one (the server's).
    /// The current file is kept next to it as a backup, whose path is returned.
    pub fn import(&self, source: &Path) -> Result<Option<PathBuf>> {
        let Some(path) = self.path.clone() else {
            return Err(AppError::internal(
                "Banco em memória não pode ser substituído",
            ));
        };
        if source.canonicalize().ok() == path.canonicalize().ok() {
            return Err(AppError::new(400, "Este já é o banco do aplicativo"));
        }
        {
            let invalid = |e: rusqlite::Error| {
                AppError::new(400, format!("Arquivo não é um banco do NF Fatura: {e}"))
            };
            let other =
                Connection::open_with_flags(source, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                    .map_err(invalid)?;
            for table in TABLES {
                let found: i64 = other
                    .query_row(
                        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                        [table],
                        |r| r.get(0),
                    )
                    .map_err(invalid)?;
                if found == 0 {
                    return Err(AppError::new(
                        400,
                        format!("Arquivo não é um banco do NF Fatura: falta a tabela {table}"),
                    ));
                }
            }
        }
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        // Close the current file before touching it.
        *conn = Connection::open_in_memory()?;
        let backup = path.with_extension(format!(
            "backup-{}.db",
            chrono::Local::now().format("%Y%m%d-%H%M%S")
        ));
        let had_data = path.exists();
        let swap = (|| -> Result<()> {
            if had_data {
                std::fs::rename(&path, &backup)?;
            }
            std::fs::copy(source, &path)?;
            Ok(())
        })();
        let reopened = Connection::open(&path)?;
        prepare(&reopened)?;
        *conn = reopened;
        swap?;
        Ok(had_data.then_some(backup))
    }
}

/// True when the statement failed on a UNIQUE constraint.
pub fn is_unique_violation(e: &rusqlite::Error) -> bool {
    matches!(e, rusqlite::Error::SqliteFailure(f, _)
        if f.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
            || f.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY)
}
