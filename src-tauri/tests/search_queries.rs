use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use everyfile_lib::domain::models::{SearchMode, SearchRequest, TermMode};
use everyfile_lib::indexing::ActivityLimiter;
use everyfile_lib::infrastructure::database::Database;
use everyfile_lib::infrastructure::secure_key::SecretKey;
use everyfile_lib::search::{ParsedQuery, SearchRegistry, SearchRepository};
use tempfile::TempDir;
use zeroize::Zeroizing;

#[test]
fn parses_combined_search_operators_without_sql_fragments() {
    let parsed =
        ParsedQuery::parse("\"중간 고사\" -정답 ext:hwp,pdf path:교육 after:2026-01-01").unwrap();

    assert_eq!(parsed.phrases, ["중간 고사"]);
    assert_eq!(parsed.excluded_terms, ["정답"]);
    assert_eq!(parsed.extensions, ["hwp", "pdf"]);
    assert_eq!(parsed.path_terms, ["교육"]);
    assert_eq!(parsed.after.as_deref(), Some("2026-01-01"));
}

#[test]
fn parses_explicit_any_term_operator() {
    let parsed = ParsedQuery::parse("중간고사 OR 수행평가").unwrap();

    assert_eq!(parsed.terms, ["중간고사", "수행평가"]);
    assert!(parsed.match_any);
    assert!(ParsedQuery::parse("OR 중간고사").is_err());
    assert!(ParsedQuery::parse("중간고사 OR").is_err());
}

#[test]
fn explicit_or_keeps_and_precedence_in_mixed_groups() {
    let fixture = Fixture::new();
    fixture.insert_document(
        "doc-alpha",
        "folder-1",
        r"C:\fixture\alpha.txt",
        "alpha.txt",
        "txt",
        "2026-01-01T00:00:00Z",
        1,
        "",
        "alpha",
    );
    fixture.insert_document(
        "doc-beta",
        "folder-1",
        r"C:\fixture\beta.txt",
        "beta.txt",
        "txt",
        "2026-01-01T00:00:00Z",
        1,
        "",
        "beta",
    );
    fixture.insert_document(
        "doc-beta-gamma",
        "folder-1",
        r"C:\fixture\beta-gamma.txt",
        "beta-gamma.txt",
        "txt",
        "2026-01-01T00:00:00Z",
        1,
        "",
        "beta gamma",
    );

    let mut explicit_any = request("alpha OR beta gamma", SearchMode::Keyword);
    explicit_any.term_mode = TermMode::Any;
    let response = fixture.repository.search(&explicit_any).unwrap();
    let ids = response
        .hits
        .iter()
        .map(|hit| hit.document_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["doc-alpha", "doc-beta-gamma"]);
}

#[test]
fn selected_term_modes_match_normalized_or_queries_and_reject_ambiguous_dtos() {
    let fixture = Fixture::new();
    for (id, body) in [
        ("doc-exact", "alpha beta"),
        ("doc-alpha", "alpha only"),
        ("doc-beta", "beta only"),
        ("doc-gamma", "gamma only"),
    ] {
        fixture.insert_document(
            id,
            "folder-1",
            &format!(r"C:\fixture\{id}.txt"),
            &format!("{id}.txt"),
            "txt",
            "2026-01-01T00:00:00Z",
            1,
            "",
            body,
        );
    }

    for (term_mode, expected) in [
        (TermMode::Exact, vec!["doc-exact"]),
        (TermMode::Near, vec!["doc-exact"]),
        (TermMode::Exclude, vec!["doc-gamma"]),
    ] {
        let mut normalized = request("alpha beta", SearchMode::Keyword);
        normalized.term_mode = term_mode;
        let response = fixture.repository.search(&normalized).unwrap();
        assert_eq!(
            response
                .hits
                .iter()
                .map(|hit| hit.document_id.as_str())
                .collect::<Vec<_>>(),
            expected
        );

        let mut ambiguous = request("alpha OR beta", SearchMode::Keyword);
        ambiguous.term_mode = term_mode;
        assert!(matches!(
            fixture.repository.search(&ambiguous),
            Err(everyfile_lib::search::SearchError::InvalidRequest(_))
        ));
    }
}

#[test]
fn quoted_extension_syntax_matches_the_duplicate_dto_filter() {
    let fixture = Fixture::new();
    fixture.insert_document(
        "doc-txt",
        "folder-1",
        r"C:\fixture\alpha.txt",
        "alpha.txt",
        "txt",
        "2026-01-01T00:00:00Z",
        1,
        "",
        "alpha",
    );
    fixture.insert_document(
        "doc-pdf",
        "folder-1",
        r"C:\fixture\alpha.pdf",
        "alpha.pdf",
        "pdf",
        "2026-01-01T00:00:00Z",
        1,
        "",
        "alpha",
    );
    let mut request = request(r#"alpha ext:"txt""#, SearchMode::Keyword);
    request.extensions = vec!["txt".into()];

    let response = fixture.repository.search(&request).unwrap();

    assert_eq!(response.total, 1);
    assert_eq!(response.hits[0].document_id, "doc-txt");
}

#[test]
fn extensionless_filter_matches_only_documents_without_an_extension() {
    let fixture = Fixture::new();
    fixture.insert_document(
        "doc-extensionless",
        "folder-1",
        r"C:\fixture\README",
        "README",
        "",
        "2026-01-01T00:00:00Z",
        1,
        "",
        "project notes",
    );
    fixture.insert_document(
        "doc-txt",
        "folder-1",
        r"C:\fixture\README.txt",
        "README.txt",
        "txt",
        "2026-01-01T00:00:00Z",
        1,
        "",
        "project notes",
    );
    let mut filtered = request("", SearchMode::Keyword);
    filtered.extensionless = true;

    let response = fixture.repository.search(&filtered).unwrap();

    assert_eq!(response.total, 1);
    assert_eq!(response.hits[0].document_id, "doc-extensionless");
    assert!(response
        .applied_filters
        .iter()
        .any(|filter| filter == "extensionless"));
}

#[test]
fn parser_handles_empty_escaped_near_korean_and_hostile_inputs() {
    let empty = ParsedQuery::parse(" \t ").unwrap();
    assert!(empty.is_empty());

    let parsed = ParsedQuery::parse(
        r#""인용 \"문장\"" 한글검색 ~7 path:"교사 자료" before:2026-12-31 '; DROP TABLE documents;--"#,
    )
    .unwrap();
    assert_eq!(parsed.phrases, [r#"인용 "문장""#]);
    assert_eq!(parsed.near, Some(7));
    assert!(parsed.terms.contains(&"한글검색".to_string()));
    assert_eq!(parsed.path_terms, ["교사 자료"]);
    assert_eq!(parsed.before.as_deref(), Some("2026-12-31"));
    assert!(parsed
        .terms
        .iter()
        .any(|term: &String| term.contains("DROP")));
}

#[test]
fn parser_preserves_quoted_windows_and_unc_path_separators() {
    let parsed =
        ParsedQuery::parse(r#"path:"C:\Program Files\EveryFile" path:"\\server\share\교육 자료""#)
            .unwrap();

    assert_eq!(
        parsed.path_terms,
        [r"C:\Program Files\EveryFile", r"\\server\share\교육 자료"]
    );
}

#[test]
fn parser_rejects_invalid_dates_extensions_and_unclosed_quotes() {
    assert!(ParsedQuery::parse("after:2026-02-30").is_err());
    assert!(ParsedQuery::parse("before:2026/01/01").is_err());
    assert!(ParsedQuery::parse("ext:pdf,../../exe").is_err());
    assert!(ParsedQuery::parse("\"unfinished").is_err());
    assert!(ParsedQuery::parse("term ~0").is_err());
    assert!(ParsedQuery::parse("term ~101").is_err());
    assert!(ParsedQuery::parse("after:2026-02-01 before:2026-01-01").is_err());
    assert!(ParsedQuery::parse("~3").is_err());
    assert!(ParsedQuery::parse("alpha ~3").is_err());
    assert!(ParsedQuery::parse("alpha beta ~3 ~4").is_err());
}

#[test]
fn keyword_search_supports_phrase_exclusion_filters_paging_and_korean() {
    let fixture = Fixture::new();
    fixture.insert_document(
        "doc-1",
        "folder-1",
        r"C:\교육\중간고사 전략.hwp",
        "중간고사 전략.hwp",
        "hwp",
        "2026-03-20T10:00:00Z",
        100,
        "중간 고사",
        "중간 고사 해설 없는 학습 전략과 한글 본문",
    );
    fixture.insert_document(
        "doc-2",
        "folder-1",
        r"C:\교육\중간고사 정답.pdf",
        "중간고사 정답.pdf",
        "pdf",
        "2026-03-21T10:00:00Z",
        200,
        "정답지",
        "중간 고사 정답 해설",
    );
    fixture.insert_document(
        "doc-3",
        "folder-2",
        r"D:\기타\중간고사.txt",
        "중간고사.txt",
        "txt",
        "2025-12-01T10:00:00Z",
        300,
        "오래된 문서",
        "중간 고사 학습 전략",
    );

    let mut request = request(
        "\"중간 고사\" -정답 path:교육 ext:hwp,pdf after:2026-01-01",
        SearchMode::Keyword,
    );
    request.folder_ids = vec!["folder-1".into()];
    request.limit = 1;

    let first = fixture.repository.search(&request).unwrap();
    assert_eq!(first.total, 1);
    assert_eq!(first.hits.len(), 1);
    assert_eq!(first.hits[0].document_id, "doc-1");
    assert!(first.hits[0].snippet.as_deref().unwrap().contains("<mark>"));
    assert!(!first.has_more);
    assert!(first
        .applied_filters
        .iter()
        .any(|filter| filter == "extension"));
}

#[test]
fn filename_search_escapes_like_metacharacters_and_honors_sort_and_page_cap() {
    let fixture = Fixture::new();
    fixture.insert_document(
        "doc-percent",
        "folder-1",
        r"C:\fixture\100%_계획.pdf",
        "100%_계획.pdf",
        "pdf",
        "2026-02-01T00:00:00Z",
        10,
        "",
        "",
    );
    fixture.insert_document(
        "doc-other",
        "folder-1",
        r"C:\fixture\100A계획.pdf",
        "100A계획.pdf",
        "pdf",
        "2026-01-01T00:00:00Z",
        20,
        "",
        "",
    );

    let mut request = request("100%_", SearchMode::Filename);
    request.sort = "newest".into();
    request.limit = 500;

    let response = fixture.repository.search(&request).unwrap();
    assert_eq!(response.total, 1);
    assert_eq!(response.hits[0].document_id, "doc-percent");
    assert!(!response.has_more);
}

#[test]
fn any_term_search_and_confidence_sort_are_supported_in_both_modes() {
    let fixture = Fixture::new();
    fixture.insert_document(
        "doc-alpha",
        "folder-1",
        r"C:\fixture\alpha.txt",
        "alpha.txt",
        "txt",
        "2026-01-01T00:00:00Z",
        1,
        "",
        "alpha only",
    );
    fixture.insert_document(
        "doc-beta",
        "folder-1",
        r"C:\fixture\beta.txt",
        "beta.txt",
        "txt",
        "2026-01-01T00:00:00Z",
        1,
        "",
        "beta only",
    );

    let mut keyword = request("alpha OR beta", SearchMode::Keyword);
    keyword.term_mode = TermMode::Any;
    keyword.sort = "confidence".into();
    let keyword_results = fixture.repository.search(&keyword).unwrap();
    assert_eq!(keyword_results.total, 2);

    let mut filename = request("alpha OR beta", SearchMode::Filename);
    filename.term_mode = TermMode::Any;
    let filename_results = fixture.repository.search(&filename).unwrap();
    assert_eq!(filename_results.total, 2);
}

#[test]
fn backend_rejects_filename_combinations_that_the_ui_disables() {
    let fixture = Fixture::new();
    let mut near = request("alpha beta", SearchMode::Filename);
    near.term_mode = everyfile_lib::domain::models::TermMode::Near;
    assert!(near_error(&fixture, near));

    let mut without_filename = request("alpha", SearchMode::Filename);
    without_filename.include_filename = false;
    assert!(near_error(&fixture, without_filename));

    let mut confidence = request("alpha", SearchMode::Filename);
    confidence.sort = "confidence".into();
    assert!(near_error(&fixture, confidence));
}

fn near_error(fixture: &Fixture, request: SearchRequest) -> bool {
    fixture.repository.search(&request).is_err()
}

#[test]
fn hostile_queries_are_bound_and_do_not_modify_the_database() {
    let fixture = Fixture::new();
    fixture.insert_document(
        "doc-1",
        "folder-1",
        r"C:\fixture\safe.txt",
        "safe.txt",
        "txt",
        "2026-01-01T00:00:00Z",
        1,
        "safe",
        "ordinary content",
    );

    for (query, mode) in [
        ("'; DROP TABLE documents; --", SearchMode::Keyword),
        ("%' OR 1=1 --", SearchMode::Filename),
    ] {
        let mut hostile = request(query, mode);
        if query.contains(" OR ") {
            hostile.term_mode = TermMode::Any;
        }
        let response = fixture.repository.search(&hostile).unwrap();
        assert_eq!(response.total, 0);
    }

    let count: i64 = fixture
        .database
        .connection()
        .query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn records_only_successful_non_private_non_empty_searches() {
    let fixture = Fixture::new();

    fixture
        .repository
        .search(&request("missing", SearchMode::Keyword))
        .unwrap();
    let mut private = request("private", SearchMode::Filename);
    private.private_search = true;
    fixture.repository.search(&private).unwrap();
    fixture
        .repository
        .search(&request("", SearchMode::Filename))
        .unwrap();
    fixture
        .repository
        .search(&request("\"\"", SearchMode::Filename))
        .unwrap();
    assert!(fixture
        .repository
        .search(&request("after:not-a-date", SearchMode::Keyword))
        .is_err());

    let connection = fixture.database.connection();
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM search_history", [], |row| row.get(0))
        .unwrap();
    let recorded: String = connection
        .query_row("SELECT query FROM search_history", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(recorded, "missing");
}

#[test]
fn keyword_search_honors_near_content_only_and_exclusion_only_queries() {
    let fixture = Fixture::new();
    fixture.insert_document(
        "doc-near",
        "folder-1",
        r"C:\fixture\filename-needle.txt",
        "filename-needle.txt",
        "txt",
        "2026-01-01T00:00:00Z",
        1,
        "lesson",
        "alpha beta ordinary",
    );
    fixture.insert_document(
        "doc-far",
        "folder-1",
        r"C:\fixture\other.txt",
        "other.txt",
        "txt",
        "2026-01-01T00:00:00Z",
        1,
        "lesson",
        "alpha one two three four beta blocked",
    );
    fixture.insert_unindexed_document(
        "doc-pending",
        "folder-1",
        r"C:\fixture\pending.txt",
        "pending.txt",
        "txt",
        "2026-01-01T00:00:00Z",
        "pending",
    );

    let mut near_request = request("alpha beta ~1", SearchMode::Keyword);
    near_request.term_mode = TermMode::Near;
    let near = fixture.repository.search(&near_request).unwrap();
    assert_eq!(near.total, 1);
    assert_eq!(near.hits[0].document_id, "doc-near");

    let filename = fixture
        .repository
        .search(&request("filename-needle", SearchMode::Keyword))
        .unwrap();
    assert_eq!(filename.total, 1);
    let mut content_only = request("filename-needle", SearchMode::Keyword);
    content_only.include_filename = false;
    assert_eq!(fixture.repository.search(&content_only).unwrap().total, 0);

    let mut excluded_request = request("-blocked", SearchMode::Keyword);
    excluded_request.term_mode = TermMode::Exclude;
    let excluded = fixture.repository.search(&excluded_request).unwrap();
    assert_eq!(excluded.total, 1);
    assert_eq!(excluded.hits[0].document_id, "doc-near");
}

#[test]
fn paging_reports_total_and_has_more() {
    let fixture = Fixture::new();
    for index in 0..3 {
        fixture.insert_document(
            &format!("doc-{index}"),
            "folder-1",
            &format!(r"C:\fixture\doc-{index}.txt"),
            &format!("doc-{index}.txt"),
            "txt",
            "2026-01-01T00:00:00Z",
            1,
            "",
            "sharedterm",
        );
    }
    let mut first_request = request("sharedterm", SearchMode::Keyword);
    first_request.limit = 1;
    let first = fixture.repository.search(&first_request).unwrap();
    assert_eq!(first.total, 3);
    assert_eq!(first.hits.len(), 1);
    assert!(first.has_more);

    first_request.offset = 2;
    let last = fixture.repository.search(&first_request).unwrap();
    assert_eq!(last.hits.len(), 1);
    assert!(!last.has_more);

    let history_count: i64 = fixture
        .database
        .connection()
        .query_row("SELECT COUNT(*) FROM search_history", [], |row| row.get(0))
        .unwrap();
    assert_eq!(history_count, 1);
}

#[test]
fn keyword_snippet_uses_the_best_matching_fts_column() {
    let fixture = Fixture::new();
    fixture.insert_document(
        "doc-title",
        "folder-1",
        r"C:\fixture\ordinary.txt",
        "ordinary.txt",
        "txt",
        "2026-01-01T00:00:00Z",
        1,
        "titleonlyneedle",
        "ordinary body",
    );
    fixture.insert_document(
        "doc-name",
        "folder-1",
        r"C:\fixture\filenameonlyneedle.txt",
        "filenameonlyneedle.txt",
        "txt",
        "2026-01-01T00:00:00Z",
        1,
        "ordinary title",
        "ordinary body",
    );

    for query in ["titleonlyneedle", "filenameonlyneedle"] {
        let response = fixture
            .repository
            .search(&request(query, SearchMode::Keyword))
            .unwrap();
        assert_eq!(response.total, 1);
        assert!(response.hits[0]
            .snippet
            .as_deref()
            .unwrap()
            .contains("<mark>"));
        assert_eq!(
            response.hits[0].match_kind,
            if query == "filenameonlyneedle" {
                everyfile_lib::domain::models::SearchMatchKind::Filename
            } else {
                everyfile_lib::domain::models::SearchMatchKind::Content
            }
        );
    }
}

#[test]
fn superseded_search_cannot_return_or_write_history_ahead_of_the_new_request() {
    let fixture = Fixture::new();
    fixture.insert_document(
        "doc-1",
        "folder-1",
        r"C:\fixture\new.txt",
        "new.txt",
        "txt",
        "2026-01-01T00:00:00Z",
        1,
        "",
        "newterm",
    );
    let database_gate = fixture.database.connection();
    let mut old_request = request("oldterm", SearchMode::Keyword);
    old_request.request_id = "old-request".into();
    let old_lease = fixture
        .repository
        .begin_request(&old_request.request_id)
        .unwrap();
    let old_repository = fixture.repository.clone();
    let (started_tx, started_rx) = mpsc::sync_channel(0);
    let (finished_tx, finished_rx) = mpsc::sync_channel(0);
    let old_worker = thread::spawn(move || {
        started_tx.send(()).unwrap();
        finished_tx
            .send(old_repository.search_registered(&old_request, old_lease))
            .unwrap();
    });

    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let mut new_request = request("newterm", SearchMode::Keyword);
    new_request.request_id = "new-request".into();
    let new_lease = fixture
        .repository
        .begin_request(&new_request.request_id)
        .unwrap();
    drop(database_gate);

    let new_response = fixture
        .repository
        .search_registered(&new_request, new_lease)
        .unwrap();
    let old_result = finished_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    old_worker.join().unwrap();

    assert_eq!(new_response.request_id, "new-request");
    assert_eq!(new_response.total, 1);
    assert!(matches!(
        old_result,
        Err(everyfile_lib::search::SearchError::Cancelled)
    ));
    let connection = fixture.database.connection();
    let history = connection
        .prepare("SELECT query FROM search_history ORDER BY searched_at")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(history, ["newterm"]);
}

fn request(query: &str, mode: SearchMode) -> SearchRequest {
    SearchRequest {
        request_id: "search-test".into(),
        query: query.into(),
        mode,
        folder_ids: Vec::new(),
        extensions: Vec::new(),
        extensionless: false,
        modified_after: None,
        modified_before: None,
        include_filename: true,
        term_mode: everyfile_lib::domain::models::TermMode::All,
        private_search: false,
        sort: "relevance".into(),
        limit: 100,
        offset: 0,
    }
}

struct Fixture {
    _temp: TempDir,
    database: Arc<Database>,
    repository: SearchRepository,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let key = SecretKey::from_bytes(Zeroizing::new([41_u8; 32]));
        let database = Arc::new(Database::open(&temp.path().join("search.db"), &key).unwrap());
        database.migrate().unwrap();
        {
            let connection = database.connection();
            connection
                .execute_batch(
                    "INSERT INTO folders
                     (id, canonical_path, display_name, created_at, enabled)
                     VALUES
                       ('folder-1', 'C:\\fixture', 'Fixture', '2026-01-01T00:00:00Z', 1),
                       ('folder-2', 'D:\\other', 'Other', '2026-01-01T00:00:00Z', 1);",
                )
                .unwrap();
        }
        let registry = SearchRegistry::new(database.interrupt_handle());
        let repository =
            SearchRepository::new(Arc::clone(&database), ActivityLimiter::default(), registry);
        Self {
            _temp: temp,
            database,
            repository,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_document(
        &self,
        id: &str,
        folder_id: &str,
        path: &str,
        file_name: &str,
        extension: &str,
        modified_at: &str,
        size_bytes: u64,
        title: &str,
        body: &str,
    ) {
        let connection = self.database.connection();
        connection
            .execute(
                "INSERT INTO documents
                 (id, folder_id, canonical_path, file_name, extension, size_bytes,
                  modified_at, parse_state, indexed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'completed', ?7)",
                rusqlite::params![
                    id,
                    folder_id,
                    path,
                    file_name,
                    extension,
                    i64::try_from(size_bytes).unwrap(),
                    modified_at
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO document_fts (document_id, file_name, title, body)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, file_name, title, body],
            )
            .unwrap();
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_unindexed_document(
        &self,
        id: &str,
        folder_id: &str,
        path: &str,
        file_name: &str,
        extension: &str,
        modified_at: &str,
        parse_state: &str,
    ) {
        self.database
            .connection()
            .execute(
                "INSERT INTO documents
                 (id, folder_id, canonical_path, file_name, extension, size_bytes,
                  modified_at, parse_state, indexed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7, NULL)",
                rusqlite::params![
                    id,
                    folder_id,
                    path,
                    file_name,
                    extension,
                    modified_at,
                    parse_state
                ],
            )
            .unwrap();
    }
}
