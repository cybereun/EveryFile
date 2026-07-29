use everyfile_lib::infrastructure::database::Database;
use everyfile_lib::infrastructure::secure_key::{SecretKey, SecureKeyStore};
use std::fs;
use tempfile::tempdir;

#[test]
fn database_reopens_with_the_same_key_and_rejects_a_different_key() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("everyfile.db");
    let key = SecretKey::from_bytes([7_u8; 32]);
    let wrong = SecretKey::from_bytes([9_u8; 32]);

    Database::open(&path, &key).unwrap().migrate().unwrap();
    assert!(Database::open(&path, &key).is_ok());
    assert!(Database::open(&path, &wrong).is_err());
}

#[test]
fn database_migration_creates_the_initial_schema_and_enables_safety_pragmas() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("everyfile.db");
    let key = SecretKey::from_bytes([7_u8; 32]);
    let database = Database::open(&path, &key).unwrap();

    database.migrate().unwrap();

    let connection = database.connection();
    let foreign_keys: i64 = connection
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .unwrap();
    let journal_mode: String = connection
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    let table_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE name IN (
               'folders', 'documents', 'document_content', 'document_fts',
               'bookmarks', 'tags', 'document_tags', 'search_history', 'index_jobs'
             )",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(foreign_keys, 1);
    assert_eq!(journal_mode, "wal");
    assert_eq!(table_count, 9);
}

#[test]
fn database_files_do_not_contain_inserted_plaintext() {
    const MARKER: &str = "EVERYFILE_CONFIDENTIAL_MARKER_6E1D19";

    let dir = tempdir().unwrap();
    let path = dir.path().join("everyfile.db");
    let key = SecretKey::from_bytes([7_u8; 32]);
    let database = Database::open(&path, &key).unwrap();
    database.migrate().unwrap();

    {
        let connection = database.connection();
        connection
            .execute(
                "INSERT INTO folders
                 (id, canonical_path, display_name, created_at, enabled)
                 VALUES ('folder-1', 'C:\\fixture', 'Fixture', '2026-07-29T00:00:00Z', 1)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO documents
                 (id, folder_id, canonical_path, file_name, extension, size_bytes,
                  modified_at, parse_state)
                 VALUES (
                   'document-1', 'folder-1', 'C:\\fixture\\secret.txt', 'secret.txt',
                   'txt', 36, '2026-07-29T00:00:00Z', 'parsed'
                 )",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO document_content
                 (document_id, title, body, markdown, blocks_json, warnings_json)
                 VALUES ('document-1', NULL, ?1, ?1, '[]', '[]')",
                [MARKER],
            )
            .unwrap();
    }

    drop(database);

    for entry in fs::read_dir(dir.path()).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            let bytes = fs::read(entry.path()).unwrap();
            assert!(
                !bytes
                    .windows(MARKER.len())
                    .any(|window| window == MARKER.as_bytes()),
                "plaintext marker leaked into {}",
                entry.path().display()
            );
        }
    }
}

#[cfg(windows)]
#[test]
fn secure_key_store_persists_only_a_dpapi_protected_blob() {
    let dir = tempdir().unwrap();

    let first = SecureKeyStore::load_or_create(dir.path()).unwrap();
    let persisted = fs::read(dir.path().join("key.dat")).unwrap();
    let second = SecureKeyStore::load_or_create(dir.path()).unwrap();

    assert_eq!(first.as_bytes(), second.as_bytes());
    assert_ne!(persisted.as_slice(), first.as_bytes());
    assert!(!persisted
        .windows(first.as_bytes().len())
        .any(|window| window == first.as_bytes()));
}
