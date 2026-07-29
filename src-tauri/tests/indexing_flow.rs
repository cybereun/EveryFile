use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use everyfile_lib::folders::repository::FolderRepository;
use everyfile_lib::indexing::{
    DocumentParser, IndexCoordinator, IndexStatus, IndexWatcher, JobId, JobState, WatchChange,
};
use everyfile_lib::infrastructure::database::Database;
use everyfile_lib::infrastructure::secure_key::SecretKey;
use everyfile_lib::parsing::{ParseErrorCode, ParsedDocument, ParserError};
use tempfile::TempDir;
use zeroize::Zeroizing;

#[derive(Default)]
struct FakeParser {
    counts: Mutex<HashMap<String, usize>>,
    damaged: Mutex<HashSet<String>>,
}

impl FakeParser {
    fn damage(&self, file_name: &str) {
        self.damaged.lock().unwrap().insert(file_name.to_owned());
    }

    fn count(&self, file_name: &str) -> usize {
        self.counts
            .lock()
            .unwrap()
            .get(file_name)
            .copied()
            .unwrap_or(0)
    }
}

impl DocumentParser for FakeParser {
    fn parse(&self, path: &Path, _max_bytes: u64) -> Result<ParsedDocument, ParserError> {
        let file_name = path.file_name().unwrap().to_string_lossy().into_owned();
        *self
            .counts
            .lock()
            .unwrap()
            .entry(file_name.clone())
            .or_default() += 1;
        thread::sleep(Duration::from_millis(25));
        if self.damaged.lock().unwrap().contains(&file_name) {
            return Err(ParserError::Protocol {
                code: ParseErrorCode::Damaged,
                message: "fixture is damaged".into(),
            });
        }
        let body = fs::read_to_string(path).unwrap();
        Ok(ParsedDocument {
            title: None,
            markdown: body.clone(),
            plain_text: body,
            blocks: vec![],
            metadata: serde_json::json!({}),
            warnings: vec![],
        })
    }
}

struct IndexHarness {
    _temp: TempDir,
    root: PathBuf,
    database: Arc<Database>,
    folder_id: String,
    parser: Arc<FakeParser>,
    coordinator: RwLock<Arc<IndexCoordinator>>,
    watcher: Mutex<Option<IndexWatcher>>,
}

impl IndexHarness {
    fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("registered");
        fs::create_dir(&root).unwrap();
        let key = SecretKey::from_bytes(Zeroizing::new([19_u8; 32]));
        let database = Arc::new(Database::open(&temp.path().join("index.db"), &key).unwrap());
        database.migrate().unwrap();
        let folder = FolderRepository::new(Arc::clone(&database))
            .register(&root)
            .unwrap();
        let parser = Arc::new(FakeParser::default());
        let coordinator = Arc::new(IndexCoordinator::with_parser(
            Arc::clone(&database),
            parser.clone(),
            10 * 1024 * 1024,
        ));
        Self {
            _temp: temp,
            root,
            database,
            folder_id: folder.id,
            parser,
            coordinator: RwLock::new(coordinator),
            watcher: Mutex::new(None),
        }
    }

    fn with_files<const N: usize>(self, names: [&str; N]) -> Self {
        for name in names {
            self.write(name, name.trim_end_matches(".txt"));
        }
        self
    }

    fn with_valid_file(self, name: &str) -> Self {
        self.write(name, name.trim_end_matches(".txt"));
        self
    }

    fn with_damaged_file(self, name: &str) -> Self {
        self.write(name, "damaged");
        self.parser.damage(name);
        self
    }

    fn write(&self, name: &str, body: &str) {
        let path = self.root.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
    }

    async fn start(&self) -> JobId {
        let coordinator = self.coordinator.read().unwrap().clone();
        coordinator.start(&self.folder_id).await.unwrap()
    }

    async fn start_with_many_files(&self, count: usize) -> JobId {
        for index in 0..count {
            self.write(&format!("bulk-{index:03}.txt"), "bulk");
        }
        self.start().await
    }

    async fn wait_until_completed_files(&self, job: &str, minimum: u64) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = self.status(job).await;
            if status.completed_files >= minimum {
                return;
            }
            assert!(Instant::now() < deadline, "timed out at {status:?}");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn wait_until_finished(&self, job: &str) -> IndexStatus {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let status = self.status(job).await;
            if matches!(
                status.state,
                JobState::Completed | JobState::Cancelled | JobState::Failed
            ) {
                return status;
            }
            assert!(Instant::now() < deadline, "timed out at {status:?}");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn simulate_process_restart(&self, job: &str) {
        let old = self.coordinator.read().unwrap().clone();
        old.pause(job).await.unwrap();
        old.shutdown_local(job).await;
        let replacement = Arc::new(IndexCoordinator::with_parser(
            Arc::clone(&self.database),
            self.parser.clone(),
            10 * 1024 * 1024,
        ));
        *self.coordinator.write().unwrap() = replacement;
    }

    async fn resume(&self, job: &str) {
        let coordinator = self.coordinator.read().unwrap().clone();
        coordinator.resume(job).await.unwrap();
    }

    async fn cancel(&self, job: &str) {
        let coordinator = self.coordinator.read().unwrap().clone();
        coordinator.cancel(job).await.unwrap();
    }

    async fn status(&self, job: &str) -> IndexStatus {
        let coordinator = self.coordinator.read().unwrap().clone();
        coordinator.status(job).await.unwrap()
    }

    fn parse_count(&self, file_name: &str) -> usize {
        self.parser.count(file_name)
    }

    async fn search(&self, query: &str) -> Vec<String> {
        let connection = self.database.connection();
        let mut statement = connection
            .prepare(
                "SELECT document_id FROM document_fts
                 WHERE document_fts MATCH ?1 ORDER BY document_id",
            )
            .unwrap();
        statement
            .query_map([query], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    async fn finish_initial_index(&self) {
        let job = self.start().await;
        assert_eq!(
            self.wait_until_finished(&job).await.state,
            JobState::Completed
        );
        let coordinator = self.coordinator.read().unwrap().clone();
        let watcher = IndexWatcher::start(coordinator, self.folder_id.clone())
            .await
            .unwrap();
        *self.watcher.lock().unwrap() = Some(watcher);
    }

    async fn emit_repeated_write_events(&self, name: &str, count: usize) {
        self.write(name, "changed");
        let watcher = self.watcher.lock().unwrap().as_ref().unwrap().clone();
        for _ in 0..count {
            watcher
                .ingest(WatchChange::Write(self.root.join(name)))
                .await
                .unwrap();
        }
        watcher.flush().await.unwrap();
    }

    async fn rename(&self, old: &str, new: &str) {
        let old_path = self.root.join(old);
        let new_path = self.root.join(new);
        fs::rename(&old_path, &new_path).unwrap();
        let watcher = self.watcher.lock().unwrap().as_ref().unwrap().clone();
        watcher
            .ingest(WatchChange::Rename {
                from: old_path,
                to: new_path,
            })
            .await
            .unwrap();
        watcher.flush().await.unwrap();
    }

    async fn delete(&self, name: &str) {
        let path = self.root.join(name);
        fs::remove_file(&path).unwrap();
        let watcher = self.watcher.lock().unwrap().as_ref().unwrap().clone();
        watcher.ingest(WatchChange::Delete(path)).await.unwrap();
        watcher.flush().await.unwrap();
    }

    async fn document(&self, name: &str) -> Option<String> {
        self.database
            .connection()
            .query_row(
                "SELECT id FROM documents WHERE folder_id = ?1 AND file_name = ?2",
                rusqlite::params![self.folder_id, name],
                |row| row.get(0),
            )
            .ok()
    }
}

#[tokio::test]
async fn interrupted_job_resumes_without_reparsing_completed_files() {
    let harness = IndexHarness::new().with_files(["a.txt", "b.txt", "c.txt"]);
    let job = harness.start().await;
    harness.wait_until_completed_files(&job, 1).await;
    harness.simulate_process_restart(&job).await;
    harness.resume(&job).await;
    let status = harness.wait_until_finished(&job).await;
    assert_eq!(status.completed_files, 3);
    assert_eq!(harness.parse_count("a.txt"), 1);
}

#[tokio::test]
async fn cancellation_and_file_failures_are_isolated() {
    let harness = IndexHarness::new()
        .with_valid_file("a.txt")
        .with_damaged_file("broken.pdf")
        .with_valid_file("c.txt");
    let job = harness.start().await;
    let status = harness.wait_until_finished(&job).await;
    assert_eq!(status.completed_files, 3);
    assert_eq!(status.errors.len(), 1);
    assert_eq!(status.errors[0].code, "DAMAGED");
    assert_eq!(harness.search("a").await.len(), 1);
    assert_eq!(harness.search("c").await.len(), 1);

    let second = harness.start_with_many_files(100).await;
    harness.cancel(&second).await;
    assert_eq!(harness.status(&second).await.state, JobState::Cancelled);
}

#[tokio::test]
async fn watcher_coalesces_writes_and_reconciles_rename_and_delete() {
    let harness = IndexHarness::new().with_valid_file("old.txt");
    harness.finish_initial_index().await;
    harness.emit_repeated_write_events("old.txt", 5).await;
    assert_eq!(harness.parse_count("old.txt"), 2);
    harness.rename("old.txt", "new.txt").await;
    assert!(harness.document("new.txt").await.is_some());
    harness.delete("new.txt").await;
    assert!(harness.document("new.txt").await.is_none());
}

#[tokio::test]
async fn watcher_cannot_parse_a_path_outside_the_registered_folder() {
    let harness = IndexHarness::new().with_valid_file("inside.txt");
    harness.finish_initial_index().await;
    let outside = harness._temp.path().join("outside.txt");
    fs::write(&outside, "private").unwrap();

    let watcher = harness.watcher.lock().unwrap().as_ref().unwrap().clone();
    watcher.ingest(WatchChange::Write(outside)).await.unwrap();
    watcher.flush().await.unwrap();

    assert_eq!(harness.parse_count("outside.txt"), 0);
}

#[tokio::test]
async fn foreground_activity_yields_background_parsing_between_files() {
    let harness = IndexHarness::new().with_files(["a.txt", "b.txt", "c.txt"]);
    let coordinator = harness.coordinator.read().unwrap().clone();
    let foreground = coordinator.activity_limiter().begin_foreground();
    let job = harness.start().await;

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(harness.status(&job).await.completed_files, 0);

    drop(foreground);
    assert_eq!(
        harness.wait_until_finished(&job).await.state,
        JobState::Completed
    );
}
