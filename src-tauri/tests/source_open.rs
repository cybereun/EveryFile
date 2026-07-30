use std::fs;
use std::sync::Arc;

use everyfile_lib::application::source_open::{resolve_indexed_source, SourceOpenError};
use everyfile_lib::infrastructure::database::Database;
use everyfile_lib::infrastructure::secure_key::SecretKey;
use tempfile::TempDir;
use zeroize::Zeroizing;

#[test]
fn resolves_only_an_existing_indexed_file_under_its_registered_root() {
    let fixture = Fixture::new();
    let source = fixture.root.join("report.pdf");
    fs::write(&source, b"fixture").unwrap();
    fixture.insert("doc-1", &source, &fixture.root);

    let resolved = resolve_indexed_source(&fixture.database, "doc-1").unwrap();

    assert_eq!(resolved, source.canonicalize().unwrap());
}

#[test]
fn rejects_unknown_documents_and_paths_outside_the_registered_root() {
    let fixture = Fixture::new();
    assert!(matches!(
        resolve_indexed_source(&fixture.database, "missing"),
        Err(SourceOpenError::NotFound)
    ));

    let outside_root = tempfile::tempdir().unwrap();
    let outside = outside_root.path().join("outside.pdf");
    fs::write(&outside, b"fixture").unwrap();
    fixture.insert("outside", &outside, &fixture.root);
    assert!(matches!(
        resolve_indexed_source(&fixture.database, "outside"),
        Err(SourceOpenError::OutsideRegisteredRoot)
    ));
}

struct Fixture {
    _temp: TempDir,
    root: std::path::PathBuf,
    database: Arc<Database>,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("documents");
        fs::create_dir(&root).unwrap();
        let key = SecretKey::from_bytes(Zeroizing::new([52_u8; 32]));
        let database = Arc::new(Database::open(&temp.path().join("open.db"), &key).unwrap());
        database.migrate().unwrap();
        Self {
            _temp: temp,
            root,
            database,
        }
    }

    fn insert(
        &self,
        document_id: &str,
        document_path: &std::path::Path,
        registered_root: &std::path::Path,
    ) {
        let connection = self.database.connection();
        connection
            .execute(
                "INSERT OR IGNORE INTO folders
                 (id, canonical_path, display_name, created_at, enabled)
                 VALUES ('folder-1', ?1, 'Documents', '2026-01-01T00:00:00Z', 1)",
                [registered_root.to_string_lossy().as_ref()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO documents
                 (id, folder_id, canonical_path, file_name, extension, size_bytes,
                  modified_at, parse_state)
                 VALUES (?1, 'folder-1', ?2, 'report.pdf', 'pdf', 7,
                         '2026-01-01T00:00:00Z', 'completed')",
                rusqlite::params![document_id, document_path.to_string_lossy().as_ref()],
            )
            .unwrap();
    }
}
