use std::path::Path;
use std::sync::Arc;

use rand::RngExt;
use rusqlite::{params, ErrorCode};
use thiserror::Error;

use crate::domain::models::FolderRecord;
use crate::infrastructure::database::Database;

#[derive(Clone)]
pub struct FolderRepository {
    database: Arc<Database>,
}

impl FolderRepository {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }

    pub fn register(&self, selected_root: &Path) -> Result<FolderRecord, FolderError> {
        let canonical_root =
            selected_root
                .canonicalize()
                .map_err(|source| FolderError::InvalidRoot {
                    path: selected_root.to_string_lossy().into_owned(),
                    source,
                })?;
        if !canonical_root.is_dir() {
            return Err(FolderError::NotDirectory(
                canonical_root.to_string_lossy().into_owned(),
            ));
        }

        let canonical_path = canonical_root.to_string_lossy().into_owned();
        let display_name = canonical_root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| canonical_path.clone());
        let id = random_id();
        let connection = self.database.connection();
        let result = connection.execute(
            "INSERT INTO folders (id, canonical_path, display_name, created_at, enabled)
             VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), 1)",
            params![id, canonical_path, display_name],
        );

        match result {
            Ok(_) => Ok(FolderRecord {
                id,
                canonical_path,
                display_name,
                document_count: 0,
                index_state: "idle".into(),
            }),
            Err(error) if is_unique_constraint(&error) => {
                Err(FolderError::AlreadyRegistered(canonical_path))
            }
            Err(error) => Err(FolderError::Database(error)),
        }
    }

    pub fn list(&self) -> Result<Vec<FolderRecord>, FolderError> {
        let connection = self.database.connection();
        let mut statement = connection
            .prepare(
                "SELECT
                   folders.id,
                   folders.canonical_path,
                   folders.display_name,
                   COUNT(documents.id),
                   CASE WHEN folders.enabled = 1 THEN 'idle' ELSE 'disabled' END
                 FROM folders
                 LEFT JOIN documents ON documents.folder_id = folders.id
                 GROUP BY folders.id
                 ORDER BY folders.display_name COLLATE NOCASE, folders.canonical_path",
            )
            .map_err(FolderError::Database)?;
        let folders = statement
            .query_map([], |row| {
                let document_count: i64 = row.get(3)?;
                Ok(FolderRecord {
                    id: row.get(0)?,
                    canonical_path: row.get(1)?,
                    display_name: row.get(2)?,
                    document_count: u64::try_from(document_count).unwrap_or(0),
                    index_state: row.get(4)?,
                })
            })
            .map_err(FolderError::Database)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(FolderError::Database)?;

        Ok(folders)
    }

    pub fn remove(&self, folder_id: &str) -> Result<(), FolderError> {
        let mut connection = self.database.connection();
        let transaction = connection.transaction().map_err(FolderError::Database)?;
        transaction
            .execute(
                "DELETE FROM document_fts
                 WHERE document_id IN (
                   SELECT id FROM documents WHERE folder_id = ?1
                 )",
                [folder_id],
            )
            .map_err(FolderError::Database)?;
        transaction
            .execute("DELETE FROM folders WHERE id = ?1", [folder_id])
            .map_err(FolderError::Database)?;
        transaction.commit().map_err(FolderError::Database)
    }
}

fn random_id() -> String {
    let bytes = rand::rng().random::<[u8; 16]>();
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn is_unique_constraint(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(inner, _)
            if inner.code == ErrorCode::ConstraintViolation
    )
}

#[derive(Debug, Error)]
pub enum FolderError {
    #[error("selected folder is unavailable: {path}")]
    InvalidRoot {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("selected path is not a directory: {0}")]
    NotDirectory(String),
    #[error("folder is already registered: {0}")]
    AlreadyRegistered(String),
    #[error("folder database operation failed")]
    Database(#[source] rusqlite::Error),
}

impl FolderError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidRoot { .. } => "FOLDER_INVALID_ROOT",
            Self::NotDirectory(_) => "FOLDER_NOT_DIRECTORY",
            Self::AlreadyRegistered(_) => "FOLDER_ALREADY_REGISTERED",
            Self::Database(_) => "FOLDER_DATABASE_ERROR",
        }
    }
}
