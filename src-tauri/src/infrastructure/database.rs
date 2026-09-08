use std::path::Path;
use std::time::Duration;

use parking_lot::{MappedMutexGuard, Mutex, MutexGuard};
use rusqlite::functions::FunctionFlags;
use rusqlite::{Connection, InterruptHandle};
use thiserror::Error;
use zeroize::Zeroizing;

use super::secure_key::SecretKey;

const INITIAL_MIGRATION: &str = include_str!("../../migrations/0001_initial.sql");
const RESUMABLE_INDEXING_MIGRATION: &str =
    include_str!("../../migrations/0002_resumable_indexing.sql");
const INDEX_JOB_RECOVERY_MIGRATION: &str =
    include_str!("../../migrations/0003_index_job_recovery.sql");
const RECONCILIATION_RUNS_MIGRATION: &str =
    include_str!("../../migrations/0004_reconciliation_runs.sql");
const PARSE_ATTEMPT_OWNERSHIP_MIGRATION: &str =
    include_str!("../../migrations/0005_parse_attempt_ownership.sql");
const LIBRARY_CONSTRAINTS_MIGRATION: &str =
    include_str!("../../migrations/0006_library_constraints.sql");
const SETTINGS_AND_MAINTENANCE_MIGRATION: &str =
    include_str!("../../migrations/0007_settings_and_maintenance.sql");
const OCR_MIGRATION: &str = include_str!("../../migrations/0008_ocr.sql");
const AI_MIGRATION: &str = include_str!("../../migrations/0009_ai.sql");
const INDEX_JOB_ORIGIN_MIGRATION: &str = include_str!("../../migrations/0010_index_job_origin.sql");

pub struct Database {
    connection: Mutex<Option<Connection>>,
    interrupt: std::sync::Arc<InterruptHandle>,
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
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(DatabaseError::Configure)?;
        connection
            .create_scalar_function(
                "everyfile_unicode_lower",
                1,
                FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
                |context| {
                    context
                        .get::<Option<String>>(0)
                        .map(|value| value.map(|text| text.to_lowercase()))
                },
            )
            .map_err(DatabaseError::Configure)?;

        let interrupt = std::sync::Arc::new(connection.get_interrupt_handle());
        Ok(Self {
            connection: Mutex::new(Some(connection)),
            interrupt,
        })
    }

    pub fn migrate(&self) -> Result<(), DatabaseError> {
        let mut connection = self.connection();
        let transaction = connection.transaction().map_err(DatabaseError::Migration)?;
        transaction
            .execute_batch(INITIAL_MIGRATION)
            .map_err(DatabaseError::Migration)?;
        transaction
            .execute_batch(RESUMABLE_INDEXING_MIGRATION)
            .map_err(DatabaseError::Migration)?;
        transaction
            .execute_batch(INDEX_JOB_RECOVERY_MIGRATION)
            .map_err(DatabaseError::Migration)?;
        transaction
            .execute_batch(RECONCILIATION_RUNS_MIGRATION)
            .map_err(DatabaseError::Migration)?;
        if !documents_have_parse_attempt_token(&transaction).map_err(DatabaseError::Migration)? {
            transaction
                .execute_batch(PARSE_ATTEMPT_OWNERSHIP_MIGRATION)
                .map_err(DatabaseError::Migration)?;
        }
        transaction
            .execute("DELETE FROM reconciliation_runs", [])
            .map_err(DatabaseError::Migration)?;
        transaction
            .execute(
                "UPDATE documents
                 SET parse_state = 'pending',
                     parse_error_code = NULL,
                     parse_attempt_token = NULL
                 WHERE parse_state = 'parsing'
                    OR parse_attempt_token IS NOT NULL",
                [],
            )
            .map_err(DatabaseError::Migration)?;
        transaction
            .execute_batch(
                "CREATE UNIQUE INDEX IF NOT EXISTS documents_parse_attempt_token_unique
                   ON documents(parse_attempt_token)
                   WHERE parse_attempt_token IS NOT NULL;",
            )
            .map_err(DatabaseError::Migration)?;
        transaction
            .execute_batch(LIBRARY_CONSTRAINTS_MIGRATION)
            .map_err(DatabaseError::Migration)?;
        transaction
            .execute_batch(SETTINGS_AND_MAINTENANCE_MIGRATION)
            .map_err(DatabaseError::Migration)?;
        transaction
            .execute_batch(OCR_MIGRATION)
            .map_err(DatabaseError::Migration)?;
        transaction
            .execute_batch(AI_MIGRATION)
            .map_err(DatabaseError::Migration)?;
        if !index_jobs_have_origin(&transaction).map_err(DatabaseError::Migration)? {
            transaction
                .execute_batch(INDEX_JOB_ORIGIN_MIGRATION)
                .map_err(DatabaseError::Migration)?;
        }
        transaction.commit().map_err(DatabaseError::Migration)
    }

    pub fn connection(&self) -> MappedMutexGuard<'_, Connection> {
        MutexGuard::map(self.connection.lock(), |connection| {
            connection
                .as_mut()
                .expect("database connection must not be used after shutdown")
        })
    }

    pub fn interrupt_handle(&self) -> std::sync::Arc<InterruptHandle> {
        std::sync::Arc::clone(&self.interrupt)
    }

    pub fn close(&self) -> Result<(), DatabaseError> {
        self.interrupt.interrupt();
        let connection = self.connection.lock().take();
        if let Some(connection) = connection {
            if let Err((connection, error)) = connection.close() {
                *self.connection.lock() = Some(connection);
                return Err(DatabaseError::Close(error));
            }
        }
        Ok(())
    }
}

fn documents_have_parse_attempt_token(connection: &Connection) -> Result<bool, rusqlite::Error> {
    let mut statement = connection.prepare("PRAGMA table_info(documents)")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        if row.get::<_, String>(1)? == "parse_attempt_token" {
            return Ok(true);
        }
    }
    Ok(false)
}

fn index_jobs_have_origin(connection: &Connection) -> Result<bool, rusqlite::Error> {
    let mut statement = connection.prepare("PRAGMA table_info(index_jobs)")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        if row.get::<_, String>(1)? == "origin" {
            return Ok(true);
        }
    }
    Ok(false)
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
    #[error("failed to close database")]
    Close(#[source] rusqlite::Error),
}
