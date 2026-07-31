use std::fs;
use std::sync::Arc;

use everyfile_lib::ai::secrets;
use everyfile_lib::infrastructure::database::Database;
use everyfile_lib::infrastructure::secure_key::SecretKey;
use tempfile::tempdir;
use zeroize::Zeroizing;

#[test]
fn provider_secret_is_recoverable_only_through_the_encrypted_database() {
    const SECRET: &str = "sk-everyfile-secret-marker-5adf8e";
    let directory = tempdir().unwrap();
    let path = directory.path().join("everyfile.db");
    let key = SecretKey::from_bytes(Zeroizing::new([37_u8; 32]));
    let database = Arc::new(Database::open(&path, &key).unwrap());
    database.migrate().unwrap();
    database
        .connection()
        .execute(
            "INSERT INTO ai_secrets (provider, secret, updated_at)
             VALUES ('openai', ?1, '2026-07-31T00:00:00Z')",
            [SECRET],
        )
        .unwrap();

    assert_eq!(secrets::read(&database, "openai").unwrap(), SECRET);
    drop(database);

    for entry in fs::read_dir(directory.path()).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            let bytes = fs::read(entry.path()).unwrap();
            assert!(
                !bytes
                    .windows(SECRET.len())
                    .any(|window| window == SECRET.as_bytes()),
                "AI secret leaked into {}",
                entry.path().display()
            );
        }
    }
}

#[test]
fn migration_restricts_secrets_and_recovers_interrupted_requests() {
    let directory = tempdir().unwrap();
    let key = SecretKey::from_bytes(Zeroizing::new([41_u8; 32]));
    let database = Database::open(&directory.path().join("everyfile.db"), &key).unwrap();
    database.migrate().unwrap();
    let connection = database.connection();
    assert!(connection
        .execute(
            "INSERT INTO ai_secrets (provider, secret, updated_at)
             VALUES ('unknown', 'secret', 'now')",
            [],
        )
        .is_err());

    connection
        .execute_batch(
            "INSERT INTO folders
             (id, canonical_path, display_name, created_at, enabled)
             VALUES ('folder-1', 'C:\\fixture', 'Fixture', 'now', 1);
             INSERT INTO documents
             (id, folder_id, canonical_path, file_name, extension, size_bytes,
              modified_at, parse_state)
             VALUES ('document-1', 'folder-1', 'C:\\fixture\\a.txt', 'a.txt',
                     'txt', 1, 'now', 'parsed');
             INSERT INTO ai_requests
             (request_id, document_id, provider, operation, state,
              remote_consent, created_at)
             VALUES ('request-1', 'document-1', 'ollama', 'summary',
                     'running', 0, 'now');",
        )
        .unwrap();
    drop(connection);
    database.migrate().unwrap();
    let state: String = database
        .connection()
        .query_row(
            "SELECT state FROM ai_requests WHERE request_id = 'request-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(state, "cancelled");
}
