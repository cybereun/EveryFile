use std::sync::Arc;
use std::time::{Duration, Instant};

use everyfile_lib::domain::models::{SearchMode, SearchRequest, TermMode};
use everyfile_lib::indexing::ActivityLimiter;
use everyfile_lib::infrastructure::database::Database;
use everyfile_lib::infrastructure::secure_key::SecretKey;
use everyfile_lib::search::{SearchRegistry, SearchRepository};
use rusqlite::params;
use tempfile::TempDir;
use zeroize::Zeroizing;

struct PerformanceLibrary {
    _temp: TempDir,
    database: Arc<Database>,
    search: SearchRepository,
}

impl PerformanceLibrary {
    fn with_documents(count: usize) -> Self {
        let temp = tempfile::tempdir().expect("performance temp directory");
        let key = SecretKey::from_bytes(Zeroizing::new([73_u8; 32]));
        let database =
            Arc::new(Database::open(&temp.path().join("performance.db"), &key).expect("database"));
        database.migrate().expect("migrations");
        {
            let mut connection = database.connection();
            connection
                .execute(
                    "INSERT INTO folders
                     (id, canonical_path, display_name, created_at, enabled)
                     VALUES ('performance', 'C:\\performance', 'Performance',
                             '2026-01-01T00:00:00Z', 1)",
                    [],
                )
                .expect("folder");
            let transaction = connection.transaction().expect("transaction");
            {
                let mut document = transaction
                    .prepare_cached(
                        "INSERT INTO documents
                         (id, folder_id, canonical_path, file_name, extension, size_bytes,
                          modified_at, parse_state, indexed_at)
                         VALUES (?1, 'performance', ?2, ?3, 'txt', 128,
                                 '2026-01-01T00:00:00Z', 'completed',
                                 '2026-01-01T00:00:00Z')",
                    )
                    .expect("document statement");
                let mut fts = transaction
                    .prepare_cached(
                        "INSERT INTO document_fts (document_id, file_name, title, body)
                         VALUES (?1, ?2, '', ?3)",
                    )
                    .expect("fts statement");
                for index in 0..count {
                    let id = format!("perf-{index:06}");
                    let file_name = if index == count / 2 {
                        format!("quarterly-needle-{index:06}.txt")
                    } else {
                        format!("document-{index:06}.txt")
                    };
                    let path = format!(r"C:\performance\{file_name}");
                    let body = if index % 1_000 == 0 {
                        "performance corpus searchable-marker"
                    } else {
                        "performance corpus ordinary"
                    };
                    document
                        .execute(params![id, path, file_name])
                        .expect("document insert");
                    fts.execute(params![id, file_name, body])
                        .expect("fts insert");
                }
            }
            transaction.commit().expect("commit");
        }
        let search = SearchRepository::new(
            Arc::clone(&database),
            ActivityLimiter::default(),
            SearchRegistry::new(database.interrupt_handle()),
        );
        Self {
            _temp: temp,
            database,
            search,
        }
    }

    fn request(&self, id: &str, query: &str, mode: SearchMode) -> SearchRequest {
        SearchRequest {
            request_id: id.into(),
            query: query.into(),
            mode,
            folder_ids: Vec::new(),
            extensions: Vec::new(),
            within_query: String::new(),
            extensionless: false,
            modified_after: None,
            modified_before: None,
            include_filename: true,
            term_mode: TermMode::All,
            private_search: true,
            sort: "relevance".into(),
            limit: 100,
            offset: 0,
        }
    }
}

#[test]
fn filename_search_returns_from_ten_thousand_documents_within_two_seconds() {
    let library = PerformanceLibrary::with_documents(10_000);
    let started = Instant::now();
    let response = library
        .search
        .search(&library.request("filename-10k", "quarterly-needle", SearchMode::Filename))
        .expect("filename search");

    assert_eq!(response.total, 1);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "10,000-document filename search took {:?}",
        started.elapsed()
    );
}

#[test]
fn at_least_ninety_five_percent_of_hundred_thousand_document_searches_finish_under_one_second() {
    let library = PerformanceLibrary::with_documents(100_000);
    let warmup = library
        .search
        .search(&library.request("warmup", "searchable-marker", SearchMode::Keyword))
        .expect("warmup search");
    assert_eq!(warmup.total, 100);

    let mut within_target = 0;
    for run in 0..20 {
        let started = Instant::now();
        let response = library
            .search
            .search(&library.request(
                &format!("search-100k-{run}"),
                "searchable-marker",
                SearchMode::Keyword,
            ))
            .expect("interactive search");
        assert_eq!(response.total, 100);
        if started.elapsed() < Duration::from_secs(1) {
            within_target += 1;
        }
    }
    assert!(
        within_target >= 19,
        "only {within_target}/20 searches met the one-second target"
    );

    // Keep the encrypted database alive for the full measurement.
    assert!(Arc::strong_count(&library.database) >= 2);
}

#[test]
fn refinement_over_hundred_thousand_documents_finishes_within_two_seconds() {
    let library = PerformanceLibrary::with_documents(100_000);
    let mut request = library.request("refine-100k", "performance", SearchMode::Keyword);
    request.within_query = "searchable-marker".into();
    let started = Instant::now();
    let response = library.search.search(&request).expect("refined search");

    assert_eq!(response.total, 100);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "100,000-document refined search took {:?}",
        started.elapsed()
    );
}
