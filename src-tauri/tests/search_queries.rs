use std::sync::Arc;

use everyfile_lib::domain::models::{SearchMode, SearchRequest};
use everyfile_lib::indexing::ActivityLimiter;
use everyfile_lib::infrastructure::database::Database;
use everyfile_lib::infrastructure::secure_key::SecretKey;
use everyfile_lib::search::{ParsedQuery, SearchRepository};
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
fn parser_rejects_invalid_dates_extensions_and_unclosed_quotes() {
    assert!(ParsedQuery::parse("after:2026-02-30").is_err());
    assert!(ParsedQuery::parse("before:2026/01/01").is_err());
    assert!(ParsedQuery::parse("ext:pdf,../../exe").is_err());
    assert!(ParsedQuery::parse("\"unfinished").is_err());
    assert!(ParsedQuery::parse("term ~0").is_err());
    assert!(ParsedQuery::parse("term ~101").is_err());
    assert!(ParsedQuery::parse("after:2026-02-01 before:2026-01-01").is_err());
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
        let response = fixture.repository.search(&request(query, mode)).unwrap();
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

    let near = fixture
        .repository
        .search(&request("alpha beta ~1", SearchMode::Keyword))
        .unwrap();
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

    let excluded = fixture
        .repository
        .search(&request("-blocked", SearchMode::Keyword))
        .unwrap();
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
}

fn request(query: &str, mode: SearchMode) -> SearchRequest {
    SearchRequest {
        query: query.into(),
        mode,
        folder_ids: Vec::new(),
        extensions: Vec::new(),
        modified_after: None,
        modified_before: None,
        include_filename: true,
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
        let repository = SearchRepository::new(Arc::clone(&database), ActivityLimiter::default());
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
}
