use std::path::Path;

use parking_lot::{Mutex, MutexGuard};
use rusqlite::Connection;
use thiserror::Error;
use zeroize::Zeroizing;

use super::secure_key::SecretKey;

const INITIAL_MIGRATION: &str = include_str!("../../migrations/0001_initial.sql");

pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    pub fn open(path: &Path, key: &SecretKey) -> Result<Self, DatabaseError> {
        let connection = Connection::open(path).map_err(DatabaseError::Open)?;
        let key_pragma = build_key_pragma(key);

        connection
            .execute_batch(key_pragma.as_str())
            .map_err(DatabaseError::Key)?;
        connection
            .query_row("SELECT COUNT(*) FROM sqlite_master", [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(DatabaseError::Key)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
            .map_err(DatabaseError::Configure)?;

        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn migrate(&self) -> Result<(), DatabaseError> {
        let mut connection = self.connection.lock();
        let transaction = connection.transaction().map_err(DatabaseError::Migration)?;
        transaction
            .execute_batch(INITIAL_MIGRATION)
            .map_err(DatabaseError::Migration)?;
        transaction.commit().map_err(DatabaseError::Migration)
    }

    pub fn connection(&self) -> MutexGuard<'_, Connection> {
        self.connection.lock()
    }
}

fn build_key_pragma(key: &SecretKey) -> Zeroizing<String> {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let mut pragma = Zeroizing::new(String::with_capacity(82));
    pragma.push_str("PRAGMA key = \"x'");
    for byte in key.as_bytes() {
        pragma.push(HEX[(byte >> 4) as usize] as char);
        pragma.push(HEX[(byte & 0x0f) as usize] as char);
    }
    pragma.push_str("'\";");
    pragma
}

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("failed to open database")]
    Open(#[source] rusqlite::Error),
    #[error("database encryption key was rejected")]
    Key(#[source] rusqlite::Error),
    #[error("failed to configure database")]
    Configure(#[source] rusqlite::Error),
    #[error("failed to apply database migration")]
    Migration(#[source] rusqlite::Error),
}
