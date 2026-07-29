use everyfile_lib::infrastructure::database::Database;
use everyfile_lib::infrastructure::secure_key::{SecretKey, SecureKeyStore};
use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;
use tempfile::tempdir;
use zeroize::Zeroizing;

#[test]
fn database_reopens_with_the_same_key_and_rejects_a_different_key() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("everyfile.db");
    let key = SecretKey::from_bytes(Zeroizing::new([7_u8; 32]));
    let wrong = SecretKey::from_bytes(Zeroizing::new([9_u8; 32]));

    Database::open(&path, &key).unwrap().migrate().unwrap();
    assert!(Database::open(&path, &key).is_ok());
    assert!(Database::open(&path, &wrong).is_err());
}

#[test]
fn database_migration_creates_the_initial_schema_and_enables_safety_pragmas() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("everyfile.db");
    let key = SecretKey::from_bytes(Zeroizing::new([7_u8; 32]));
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
    let key = SecretKey::from_bytes(Zeroizing::new([7_u8; 32]));
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

#[test]
fn migration_upgrades_an_existing_initial_schema_without_losing_jobs() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("legacy.db");
    let key = SecretKey::from_bytes(Zeroizing::new([7_u8; 32]));
    let database = Database::open(&path, &key).unwrap();
    {
        let connection = database.connection();
        connection
            .execute_batch(
                "CREATE TABLE folders (
                   id TEXT PRIMARY KEY,
                   canonical_path TEXT NOT NULL UNIQUE,
                   display_name TEXT NOT NULL,
                   created_at TEXT NOT NULL,
                   enabled INTEGER NOT NULL DEFAULT 1
                 );
                 CREATE TABLE index_jobs (
                   id TEXT PRIMARY KEY,
                   folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
                   state TEXT NOT NULL,
                   completed_files INTEGER NOT NULL DEFAULT 0,
                   total_files INTEGER NOT NULL DEFAULT 0,
                   last_path TEXT,
                   updated_at TEXT NOT NULL
                 );
                 INSERT INTO folders VALUES (
                   'folder-1', 'C:\\fixture', 'Fixture',
                   '2026-07-29T00:00:00Z', 1
                 );
                 INSERT INTO index_jobs VALUES (
                   'job-1', 'folder-1', 'paused', 1, 3, 'a.txt',
                   '2026-07-29T00:00:00Z'
                 );",
            )
            .unwrap();
    }

    database.migrate().unwrap();

    let connection = database.connection();
    let job_state: String = connection
        .query_row(
            "SELECT state FROM index_jobs WHERE id = 'job-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let indexing_table_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table'
               AND name IN ('index_job_files', 'index_job_errors')",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(job_state, "paused");
    assert_eq!(indexing_table_count, 2);
}

#[cfg(windows)]
#[test]
fn secure_key_store_persists_only_a_dpapi_protected_blob() {
    let dir = tempdir().unwrap();

    let first = SecureKeyStore::load_or_create(dir.path()).unwrap();
    let persisted = fs::read(dir.path().join("key.dat")).unwrap();
    let second = SecureKeyStore::load_or_create(dir.path()).unwrap();

    assert!(first.as_bytes() == second.as_bytes());
    assert!(persisted.as_slice() != first.as_bytes());
    assert!(!persisted
        .windows(first.as_bytes().len())
        .any(|window| window == first.as_bytes()));
}

#[cfg(windows)]
#[test]
fn concurrent_secure_key_creation_returns_the_single_persisted_key() {
    const CALLER_COUNT: usize = 32;

    let dir = tempdir().unwrap();
    let app_data_dir = Arc::new(dir.path().to_path_buf());
    let start = Arc::new(Barrier::new(CALLER_COUNT));
    let mut callers = Vec::with_capacity(CALLER_COUNT);

    for _ in 0..CALLER_COUNT {
        let app_data_dir = Arc::clone(&app_data_dir);
        let start = Arc::clone(&start);
        callers.push(thread::spawn(move || {
            start.wait();
            SecureKeyStore::load_or_create(&app_data_dir)
        }));
    }

    let keys: Vec<_> = callers
        .into_iter()
        .map(|caller| caller.join().unwrap().unwrap())
        .collect();
    let persisted = SecureKeyStore::load_or_create(&app_data_dir).unwrap();

    assert!(keys.iter().all(|key| key.as_bytes() == keys[0].as_bytes()));
    assert!(persisted.as_bytes() == keys[0].as_bytes());
}

#[cfg(windows)]
#[test]
fn secure_key_creation_ignores_an_abandoned_temp_file() {
    const ABANDONED_CONTENT: &[u8] = b"incomplete protected blob";

    let dir = tempdir().unwrap();
    let abandoned = dir.path().join(".key.dat.4242.0.tmp");
    fs::write(&abandoned, ABANDONED_CONTENT).unwrap();

    let created = SecureKeyStore::load_or_create(dir.path()).unwrap();
    let reloaded = SecureKeyStore::load_or_create(dir.path()).unwrap();

    assert!(created.as_bytes() == reloaded.as_bytes());
    assert!(dir.path().join("key.dat").is_file());
    assert_eq!(fs::read(abandoned).unwrap(), ABANDONED_CONTENT);
}
