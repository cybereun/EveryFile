use std::fs;
use std::sync::Arc;

use everyfile_lib::application::source_open::{
    resolve_indexed_source, verify_indexed_source, SourceOpenError,
};
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

#[test]
fn resolves_paths_with_spaces_commas_and_unicode() {
    let fixture = Fixture::new();
    let source = fixture.root.join("검토 문서, 최종본.pdf");
    fs::write(&source, b"fixture").unwrap();
    fixture.insert("doc-unicode", &source, &fixture.root);

    let resolved = resolve_indexed_source(&fixture.database, "doc-unicode").unwrap();

    assert_eq!(resolved, source.canonicalize().unwrap());
}

#[test]
fn rejects_disabled_missing_and_non_file_sources() {
    let fixture = Fixture::new();
    let source = fixture.root.join("report.pdf");
    fs::write(&source, b"fixture").unwrap();
    fixture.insert("doc-disabled", &source, &fixture.root);
    fixture
        .database
        .connection()
        .execute("UPDATE folders SET enabled = 0 WHERE id = 'folder-1'", [])
        .unwrap();
    assert!(matches!(
        resolve_indexed_source(&fixture.database, "doc-disabled"),
        Err(SourceOpenError::DisabledFolder)
    ));

    fixture
        .database
        .connection()
        .execute("UPDATE folders SET enabled = 1 WHERE id = 'folder-1'", [])
        .unwrap();
    fs::remove_file(&source).unwrap();
    assert!(matches!(
        resolve_indexed_source(&fixture.database, "doc-disabled"),
        Err(SourceOpenError::Unavailable(_))
    ));

    let directory = fixture.root.join("not-a-file");
    fs::create_dir(&directory).unwrap();
    fixture.insert("doc-directory", &directory, &fixture.root);
    assert!(matches!(
        resolve_indexed_source(&fixture.database, "doc-directory"),
        Err(SourceOpenError::NotFile)
    ));
}

#[cfg(windows)]
#[test]
fn verified_source_tracks_the_open_identity_if_paths_are_renamed_or_replaced() {
    let fixture = Fixture::new();
    let source = fixture.root.join("locked.pdf");
    let renamed = fixture.root.join("renamed.pdf");
    fs::write(&source, b"fixture").unwrap();
    fixture.insert("doc-locked", &source, &fixture.root);

    let verified = verify_indexed_source(&fixture.database, "doc-locked").unwrap();
    match fs::rename(&source, &renamed) {
        Ok(()) => {
            fs::write(&source, b"replacement").unwrap();
            assert_eq!(
                verified.current_path().unwrap(),
                renamed.canonicalize().unwrap()
            );
        }
        Err(_) => assert_eq!(
            verified.current_path().unwrap(),
            source.canonicalize().unwrap()
        ),
    }
    drop(verified);
}

#[cfg(windows)]
#[test]
fn verified_source_tracks_the_registered_root_identity_if_its_path_is_replaced() {
    let fixture = Fixture::new();
    let source = fixture.root.join("inside.pdf");
    let moved_root = fixture._temp.path().join("moved-root");
    fs::write(&source, b"fixture").unwrap();
    fixture.insert("doc-root-swap", &source, &fixture.root);

    let verified = verify_indexed_source(&fixture.database, "doc-root-swap").unwrap();
    match fs::rename(&fixture.root, &moved_root) {
        Ok(()) => {
            fs::create_dir(&fixture.root).unwrap();
            fs::write(fixture.root.join("inside.pdf"), b"replacement").unwrap();
            assert_eq!(
                verified.current_path().unwrap(),
                moved_root.join("inside.pdf").canonicalize().unwrap()
            );
        }
        Err(_) => assert_eq!(
            verified.current_path().unwrap(),
            source.canonicalize().unwrap()
        ),
    }
}

#[cfg(windows)]
#[test]
fn rejects_name_surrogate_symlinks_when_creation_is_available() {
    use std::os::windows::fs::symlink_file;

    let fixture = Fixture::new();
    let target = fixture.root.join("target.pdf");
    let link = fixture.root.join("linked.pdf");
    fs::write(&target, b"fixture").unwrap();
    if let Err(error) = symlink_file(&target, &link) {
        if error.kind() == std::io::ErrorKind::PermissionDenied
            || error.raw_os_error() == Some(1314)
        {
            return;
        }
        panic!("failed to create test symlink: {error}");
    }
    fixture.insert("doc-link", &link, &fixture.root);

    assert!(matches!(
        verify_indexed_source(&fixture.database, "doc-link"),
        Err(SourceOpenError::RedirectingReparsePoint)
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
