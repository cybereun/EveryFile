use std::fs;
use std::sync::Arc;

use everyfile_lib::diagnostics::{
    remove_app_data_contents, require_reset_confirmation, validate_reset_target, DiagnosticEvent,
    DiagnosticsLogger,
};
use everyfile_lib::domain::models::{AppSettings, SearchHit, SearchMatchKind};
use everyfile_lib::export::{export_to_destination, ExportFormat, ExportOutcome, ExportRequest};
use everyfile_lib::infrastructure::database::Database;
use everyfile_lib::infrastructure::secure_key::SecretKey;
use everyfile_lib::settings::SettingsRepository;
use everyfile_lib::statistics::StatisticsRepository;
use tempfile::TempDir;
use zeroize::Zeroizing;

#[test]
fn statistics_history_retention_and_private_rows_are_local_and_correct() {
    let fixture = Fixture::new();
    fixture.seed_documents();
    {
        let connection = fixture.database.connection();
        connection
            .execute_batch(
                "INSERT INTO search_history
                   (id, query, mode, filters_json, result_count, elapsed_ms, searched_at, private)
                 VALUES
                   ('h-old', 'old', 'keyword', '{}', 0, 1,
                    strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-91 days'), 0),
                   ('h-kept', 'kept', 'keyword', '{}', 2, 1,
                    strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-89 days'), 0),
                   ('h-repeat', 'kept', 'keyword', '{}', 2, 1,
                    strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-1 day'), 0),
                   ('h-private', 'secret', 'keyword', '{}', 2, 1,
                    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), 1);",
            )
            .unwrap();
    }

    fixture.statistics.run_history_retention(90).unwrap();
    let stats = fixture.statistics.get_statistics().unwrap();

    assert_eq!(stats.total_documents, 3);
    assert_eq!(stats.indexed_documents, 2);
    assert_eq!(stats.total_bytes, 72);
    assert_eq!(
        stats
            .by_extension
            .iter()
            .find(|bucket| bucket.label == "pdf")
            .unwrap()
            .count,
        2
    );
    assert_eq!(
        stats
            .by_folder
            .iter()
            .find(|bucket| bucket.id == "folder")
            .unwrap()
            .count,
        3
    );
    assert_eq!(stats.total_searches, 2);
    assert_eq!(stats.unique_search_terms, 1);
    assert_eq!(stats.frequent_searches[0].query, "kept");
    assert_eq!(stats.frequent_searches[0].count, 2);

    let history = fixture.statistics.list_search_history(20, 0).unwrap();
    assert_eq!(history.len(), 2);
    assert!(history.iter().all(|row| !row.private_search));
    assert!(!history.iter().any(|row| row.query == "old"));
    assert!(!history.iter().any(|row| row.query == "secret"));
}

#[test]
fn statistics_json_uses_decimal_strings_for_exact_integer_fields() {
    let fixture = Fixture::new();
    fixture.seed_documents();

    let value = serde_json::to_value(fixture.statistics.get_statistics().unwrap()).unwrap();

    assert!(value["totalDocuments"].is_string());
    assert!(value["indexedDocuments"].is_string());
    assert!(value["totalBytes"].is_string());
    assert!(value["totalSearches"].is_string());
    assert!(value["uniqueSearchTerms"].is_string());
    assert!(value["byExtension"][0]["count"].is_string());
    assert!(value["byFolder"][0]["count"].is_string());
    assert!(value["recentlyModified"][0]["sizeBytes"].is_string());
}

#[test]
fn history_can_be_deleted_cleared_and_unlimited_retention_keeps_rows() {
    let fixture = Fixture::new();
    fixture.insert_history("one", "2020-01-01T00:00:00Z");
    fixture.insert_history("two", "2020-01-02T00:00:00Z");

    fixture.statistics.run_history_retention(0).unwrap();
    let rows = fixture.statistics.list_search_history(1_000, 0).unwrap();
    assert_eq!(rows.len(), 2);
    fixture
        .statistics
        .delete_search_history(&rows[0].id)
        .unwrap();
    assert_eq!(
        fixture
            .statistics
            .list_search_history(100, 0)
            .unwrap()
            .len(),
        1
    );
    fixture.statistics.clear_search_history().unwrap();
    assert!(fixture
        .statistics
        .list_search_history(100, 0)
        .unwrap()
        .is_empty());
}

#[test]
fn exports_quote_csv_preserve_xlsx_numbers_and_use_atomic_sibling_writes() {
    let temp = tempfile::tempdir().unwrap();
    let hit = SearchHit {
        document_id: "doc-1".into(),
        file_name: "a,b.pdf".into(),
        path: r"C:\Documents\a,b.pdf".into(),
        extension: "pdf".into(),
        size_bytes: 42,
        modified_at: "2026-07-30T00:00:00Z".into(),
        snippet: Some("quoted \"excerpt\"".into()),
        score: 1.5,
        match_kind: SearchMatchKind::Both,
    };
    let request = ExportRequest::SearchResults {
        hits: vec![hit.clone()],
    };

    let csv_path = temp.path().join("results.csv");
    assert_eq!(
        export_to_destination(&request, ExportFormat::Csv, Some(&csv_path)).unwrap(),
        ExportOutcome::Written
    );
    let csv = fs::read_to_string(&csv_path).unwrap();
    assert!(csv.contains("\"a,b.pdf\""));
    assert!(csv.contains("\"quoted \"\"excerpt\"\"\""));
    assert!(!temp.path().join(".results.csv.tmp").exists());

    let xlsx_path = temp.path().join("results.xlsx");
    assert_eq!(
        export_to_destination(&request, ExportFormat::Xlsx, Some(&xlsx_path)).unwrap(),
        ExportOutcome::Written
    );
    let workbook = fs::read(&xlsx_path).unwrap();
    assert!(workbook.starts_with(b"PK"));
    assert!(workbook.len() > 1_000);

    let markdown_path = temp.path().join("document.md");
    assert_eq!(
        export_to_destination(
            &ExportRequest::MarkdownDocument {
                file_name: "a,b.pdf".into(),
                markdown: "# Heading\n\nBody".into(),
            },
            ExportFormat::Markdown,
            Some(&markdown_path),
        )
        .unwrap(),
        ExportOutcome::Written
    );
    assert_eq!(
        fs::read_to_string(markdown_path).unwrap(),
        "# Heading\n\nBody"
    );
}

#[test]
fn exporting_results_never_changes_the_indexed_source_fixture() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.txt");
    let original = b"immutable source document\n";
    fs::write(&source, original).unwrap();
    let request = ExportRequest::SearchResults {
        hits: vec![SearchHit {
            document_id: "source-lock".into(),
            file_name: "source.txt".into(),
            path: source.to_string_lossy().into_owned(),
            extension: "txt".into(),
            size_bytes: original.len() as u64,
            modified_at: "2026-07-31T00:00:00Z".into(),
            snippet: Some("immutable source document".into()),
            score: 1.0,
            match_kind: SearchMatchKind::Content,
        }],
    };

    export_to_destination(
        &request,
        ExportFormat::Csv,
        Some(&temp.path().join("results.csv")),
    )
    .unwrap();
    export_to_destination(
        &request,
        ExportFormat::Xlsx,
        Some(&temp.path().join("results.xlsx")),
    )
    .unwrap();

    assert_eq!(fs::read(source).unwrap(), original);
}

#[test]
fn csv_escapes_formulae_after_leading_whitespace_and_control_characters() {
    let temp = tempfile::tempdir().unwrap();
    let request = ExportRequest::SearchResults {
        hits: vec![SearchHit {
            document_id: "doc-formula".into(),
            file_name: "safe.txt".into(),
            path: r"C:\Documents\safe.txt".into(),
            extension: "txt".into(),
            size_bytes: 1,
            modified_at: "2026-07-30T00:00:00Z".into(),
            snippet: Some(" \n\t=HYPERLINK(\"https://example.invalid\")".into()),
            score: 1.0,
            match_kind: SearchMatchKind::Content,
        }],
    };
    let destination = temp.path().join("formula.csv");

    export_to_destination(&request, ExportFormat::Csv, Some(&destination)).unwrap();

    let csv = fs::read_to_string(destination).unwrap();
    assert!(
        csv.contains("' \n\t=HYPERLINK"),
        "dangerous excerpt must be prefixed with an apostrophe before CSV quoting"
    );
}

#[test]
fn cancelled_and_invalid_exports_create_no_file() {
    let temp = tempfile::tempdir().unwrap();
    let request = ExportRequest::SearchResults { hits: Vec::new() };

    assert_eq!(
        export_to_destination(&request, ExportFormat::Csv, None).unwrap(),
        ExportOutcome::Cancelled
    );
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    assert!(export_to_destination(
        &request,
        ExportFormat::Csv,
        Some(&temp.path().join("wrong.xlsx"))
    )
    .is_err());
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn settings_are_validated_and_persisted_in_the_encrypted_database() {
    let fixture = Fixture::new();
    let repository = SettingsRepository::new(Arc::clone(&fixture.database));
    let mut settings = AppSettings {
        language: "en".into(),
        history_retention_days: 365,
        ..AppSettings::default()
    };
    repository.save(&settings).unwrap();
    assert_eq!(repository.load().unwrap().language, "en");

    settings.history_retention_days = 31;
    assert!(repository.save(&settings).is_err());
    assert_eq!(repository.load().unwrap().history_retention_days, 365);

    for supported in ["minimizeToTray", "startWithWindows", "startHidden"] {
        let mut settings = AppSettings::default();
        match supported {
            "minimizeToTray" => settings.minimize_to_tray = true,
            "startWithWindows" => settings.start_with_windows = true,
            "startHidden" => settings.start_hidden = true,
            _ => unreachable!(),
        }
        let saved = repository.save(&settings).unwrap();
        assert_eq!(saved.minimize_to_tray, settings.minimize_to_tray);
        assert_eq!(saved.start_with_windows, settings.start_with_windows);
        assert_eq!(saved.start_hidden, settings.start_hidden);
    }

    fixture
        .database
        .connection()
        .execute(
            "UPDATE app_settings
             SET settings_json =
               '{\"language\":\"ko\",\"theme\":\"light\",\"historyRetentionDays\":90,
                 \"minimizeToTray\":false,\"startWithWindows\":false,\"startHidden\":false,
                 \"maxFileSizeBytes\":209715200,\"resultPageSize\":100}'
             WHERE id = 1",
            [],
        )
        .unwrap();
    let upgraded = repository.load().unwrap();
    assert_eq!(upgraded.file_click_behavior, "preview");
    assert_eq!(upgraded.indexing_intensity, "balanced");
}

#[test]
fn startup_flags_are_preserved_when_loading_existing_settings() {
    let fixture = Fixture::new();
    let repository = SettingsRepository::new(Arc::clone(&fixture.database));
    fixture
        .database
        .connection()
        .execute(
            "INSERT INTO app_settings (id, settings_json, updated_at)
             VALUES (
               1,
               '{\"language\":\"ko\",\"theme\":\"light\",\"historyRetentionDays\":90,
                 \"minimizeToTray\":true,\"startWithWindows\":true,\"startHidden\":true,
                 \"maxFileSizeBytes\":209715200,\"resultPageSize\":100}',
               '2026-07-30T00:00:00Z'
             )",
            [],
        )
        .unwrap();

    let loaded = repository.load().unwrap();

    assert!(loaded.minimize_to_tray);
    assert!(loaded.start_with_windows);
    assert!(loaded.start_hidden);
    let stored = fixture
        .database
        .connection()
        .query_row(
            "SELECT settings_json FROM app_settings WHERE id = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap();
    let stored: serde_json::Value = serde_json::from_str(&stored).unwrap();
    assert_eq!(stored["minimizeToTray"], true);
    assert_eq!(stored["startWithWindows"], true);
    assert_eq!(stored["startHidden"], true);
}

#[test]
fn parse_errors_are_listed_and_retry_only_changes_failed_documents() {
    let fixture = Fixture::new();
    fixture.seed_documents();
    let errors = fixture.statistics.list_parse_errors().unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].document_id, "doc-failed");
    assert!(fixture.statistics.retry_parse("doc-failed").unwrap());
    assert!(!fixture.statistics.retry_parse("doc-ok").unwrap());
    assert_eq!(
        fixture
            .database
            .connection()
            .query_row(
                "SELECT parse_state FROM documents WHERE id = 'doc-failed'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "pending"
    );
}

#[test]
fn diagnostics_redact_roots_retain_locally_and_reset_never_escapes_app_data() {
    let temp = tempfile::tempdir().unwrap();
    let local_appdata = temp.path().join("Local");
    let app_data = local_appdata.join("com.cybereun.everyfile");
    let root = temp.path().join("Private Documents");
    fs::create_dir_all(&app_data).unwrap();
    fs::create_dir_all(&root).unwrap();
    let logs = app_data.join("logs");
    fs::create_dir_all(&logs).unwrap();
    let expired_log = logs.join("diagnostics-1.jsonl");
    fs::write(&expired_log, b"{\"message\":\"expired\"}\n").unwrap();
    let unicode_root = std::path::PathBuf::from(r"C:\비밀 문서");
    let logger =
        DiagnosticsLogger::new(&app_data, vec![root.clone(), unicode_root.clone()]).unwrap();
    assert!(!expired_log.exists());
    logger
        .write(&DiagnosticEvent {
            level: "error".into(),
            code: "PARSE_FAILED".into(),
            message: format!("Could not parse {}", root.join("secret.pdf").display()),
            document_id: Some("doc-1".into()),
        })
        .unwrap();
    let log = fs::read_to_string(logger.current_log_path()).unwrap();
    assert!(log.contains("[REGISTERED_ROOT]"));
    assert!(!log.contains("Private Documents"));
    assert_eq!(
        logger.redact(r"c:\비밀 문서\성적표.pdf"),
        r"[REGISTERED_ROOT]\성적표.pdf"
    );

    for _ in 0..70 {
        logger
            .write(&DiagnosticEvent {
                level: "info".into(),
                code: "BOUNDED_LOG".into(),
                message: "x".repeat(16_000),
                document_id: None,
            })
            .unwrap();
    }
    let log_files = fs::read_dir(logger.log_directory())
        .unwrap()
        .map(|entry| entry.unwrap())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("diagnostics-")
        })
        .collect::<Vec<_>>();
    assert!(log_files.len() <= 3);
    assert!(log_files
        .iter()
        .all(|entry| entry.metadata().unwrap().len() <= 1024 * 1024));

    assert_eq!(
        validate_reset_target(&local_appdata, &app_data).unwrap(),
        app_data.canonicalize().unwrap()
    );
    assert!(validate_reset_target(&local_appdata, temp.path()).is_err());
    assert!(validate_reset_target(&local_appdata, &app_data.join("..")).is_err());
    assert!(require_reset_confirmation(false).is_err());
    require_reset_confirmation(true).unwrap();

    fs::write(app_data.join("marker"), b"local-only").unwrap();
    remove_app_data_contents(&local_appdata, &app_data).unwrap();
    assert!(app_data.exists());
    assert_eq!(fs::read_dir(&app_data).unwrap().count(), 0);
    assert!(temp.path().exists());
}

#[test]
fn diagnostics_redaction_prefers_longest_root_and_normalizes_windows_spellings() {
    let temp = tempfile::tempdir().unwrap();
    let app_data = temp.path().join("com.cybereun.everyfile");
    fs::create_dir_all(&app_data).unwrap();
    let logger = DiagnosticsLogger::new(
        &app_data,
        vec![
            std::path::PathBuf::from(r"C:\Users"),
            std::path::PathBuf::from(r"C:\Users\Alice\Private"),
            std::path::PathBuf::from(r"C:\Ä\문서"),
        ],
    )
    .unwrap();

    let redacted = logger.redact(r"\\?\C:/Users/Alice/Private/report.pdf and c:\ä\문서\secret.pdf");

    assert_eq!(
        redacted,
        "[REGISTERED_ROOT]/report.pdf and [REGISTERED_ROOT]\\secret.pdf"
    );
    assert!(!redacted.contains("Alice"));
    assert!(!redacted.contains('ä'));
}

#[cfg(windows)]
#[test]
fn reset_rejects_a_reparse_root_and_preserves_the_junction_victim() {
    use std::os::windows::process::CommandExt;

    let temp = tempfile::tempdir().unwrap();
    let local_appdata = temp.path().join("Local");
    let victim = local_appdata.join("Victim");
    let app_data = local_appdata.join("com.cybereun.everyfile");
    fs::create_dir_all(&victim).unwrap();
    fs::write(victim.join("must-survive"), b"victim").unwrap();
    let status = std::process::Command::new("cmd")
        .args([
            "/C",
            "mklink",
            "/J",
            &app_data.to_string_lossy(),
            &victim.to_string_lossy(),
        ])
        .creation_flags(0x0800_0000)
        .status()
        .unwrap();
    assert!(status.success());

    assert!(validate_reset_target(&local_appdata, &app_data).is_err());
    assert!(remove_app_data_contents(&local_appdata, &app_data).is_err());
    assert_eq!(fs::read(victim.join("must-survive")).unwrap(), b"victim");

    fs::remove_dir(&app_data).unwrap();
}

struct Fixture {
    _temp: TempDir,
    database: Arc<Database>,
    statistics: StatisticsRepository,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let key = SecretKey::from_bytes(Zeroizing::new([83_u8; 32]));
        let database = Arc::new(Database::open(&temp.path().join("stats.db"), &key).unwrap());
        database.migrate().unwrap();
        let statistics = StatisticsRepository::new(Arc::clone(&database));
        Self {
            _temp: temp,
            database,
            statistics,
        }
    }

    fn seed_documents(&self) {
        self.database
            .connection()
            .execute_batch(
                "INSERT INTO folders
                   (id, canonical_path, display_name, created_at, enabled)
                 VALUES ('folder', 'C:\\Documents', 'Documents', '2026-01-01T00:00:00Z', 1);
                 INSERT INTO documents
                   (id, folder_id, canonical_path, file_name, extension, size_bytes,
                    modified_at, parse_state, parse_error_code, indexed_at)
                 VALUES
                   ('doc-ok', 'folder', 'C:\\Documents\\one.pdf', 'one.pdf', 'pdf', 42,
                    '2026-07-29T00:00:00Z', 'parsed', NULL, '2026-07-29T00:00:00Z'),
                   ('doc-failed', 'folder', 'C:\\Documents\\two.pdf', 'two.pdf', 'pdf', 20,
                    '2026-07-28T00:00:00Z', 'failed', 'DAMAGED', NULL),
                   ('doc-text', 'folder', 'C:\\Documents\\three.txt', 'three.txt', 'txt', 10,
                    '2026-07-27T00:00:00Z', 'completed', NULL, '2026-07-27T00:00:00Z');",
            )
            .unwrap();
    }

    fn insert_history(&self, query: &str, searched_at: &str) {
        self.database
            .connection()
            .execute(
                "INSERT INTO search_history
                   (id, query, mode, filters_json, result_count, elapsed_ms, searched_at, private)
                 VALUES (?1, ?2, 'keyword', '{}', 0, 1, ?3, 0)",
                rusqlite::params![format!("history-{query}"), query, searched_at],
            )
            .unwrap();
    }
}
