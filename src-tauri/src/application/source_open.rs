use std::path::{Path, PathBuf};
use std::process::Command;

use rusqlite::OptionalExtension;
use thiserror::Error;

use crate::infrastructure::database::Database;

pub fn resolve_indexed_source(
    database: &Database,
    document_id: &str,
) -> Result<PathBuf, SourceOpenError> {
    if document_id.trim().is_empty() {
        return Err(SourceOpenError::NotFound);
    }
    let (source, registered_root) = database
        .connection()
        .query_row(
            "SELECT d.canonical_path, f.canonical_path
             FROM documents d
             JOIN folders f ON f.id = d.folder_id
             WHERE d.id = ?1 AND f.enabled = 1",
            [document_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
        .ok_or(SourceOpenError::NotFound)?;

    let root = Path::new(&registered_root)
        .canonicalize()
        .map_err(SourceOpenError::Unavailable)?;
    let source = Path::new(&source)
        .canonicalize()
        .map_err(SourceOpenError::Unavailable)?;
    if !source.is_file() {
        return Err(SourceOpenError::NotFile);
    }
    if !source.starts_with(&root) {
        return Err(SourceOpenError::OutsideRegisteredRoot);
    }
    Ok(source)
}

pub fn open_indexed_source(database: &Database, document_id: &str) -> Result<(), SourceOpenError> {
    let source = resolve_indexed_source(database, document_id)?;
    launch_source(&source)
}

#[cfg(windows)]
fn launch_source(source: &Path) -> Result<(), SourceOpenError> {
    Command::new("explorer.exe")
        .arg(source)
        .spawn()
        .map(|_| ())
        .map_err(SourceOpenError::Launch)
}

#[cfg(target_os = "macos")]
fn launch_source(source: &Path) -> Result<(), SourceOpenError> {
    Command::new("open")
        .arg(source)
        .spawn()
        .map(|_| ())
        .map_err(SourceOpenError::Launch)
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn launch_source(source: &Path) -> Result<(), SourceOpenError> {
    Command::new("xdg-open")
        .arg(source)
        .spawn()
        .map(|_| ())
        .map_err(SourceOpenError::Launch)
}

#[derive(Debug, Error)]
pub enum SourceOpenError {
    #[error("indexed document was not found")]
    NotFound,
    #[error("indexed source is unavailable")]
    Unavailable(#[source] std::io::Error),
    #[error("indexed source is not a file")]
    NotFile,
    #[error("indexed source resolved outside its registered folder")]
    OutsideRegisteredRoot,
    #[error("failed to open indexed source")]
    Launch(#[source] std::io::Error),
    #[error("source lookup failed")]
    Database(#[from] rusqlite::Error),
}

impl SourceOpenError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "SOURCE_NOT_FOUND",
            Self::Unavailable(_) => "SOURCE_UNAVAILABLE",
            Self::NotFile => "SOURCE_NOT_FILE",
            Self::OutsideRegisteredRoot => "SOURCE_OUTSIDE_REGISTERED_ROOT",
            Self::Launch(_) => "SOURCE_OPEN_FAILED",
            Self::Database(_) => "SOURCE_LOOKUP_FAILED",
        }
    }
}
