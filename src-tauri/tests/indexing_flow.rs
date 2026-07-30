use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use everyfile_lib::folders::repository::FolderRepository;
use everyfile_lib::indexing::{
    DiscoveryProbe, DocumentParser, IndexCoordinator, IndexStatus, IndexWatcher, JobId, JobState,
    WatchChange,
};
use everyfile_lib::infrastructure::database::Database;
use everyfile_lib::infrastructure::secure_key::SecretKey;
use everyfile_lib::parsing::{ParseErrorCode, ParsedDocument, ParserError};
use everyfile_lib::state::AppState;
use tempfile::TempDir;
use zeroize::Zeroizing;

struct FakeParser {
    counts: Mutex<HashMap<String, usize>>,
    damaged: Mutex<HashSet<String>>,
    delay: Mutex<Duration>,
}

impl Default for FakeParser {
    fn default() -> Self {
        Self {
            counts: Mutex::new(HashMap::new()),
            damaged: Mutex::new(HashSet::new()),
            delay: Mutex::new(Duration::from_millis(25)),
        }
    }
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

    fn set_delay(&self, delay: Duration) {
        *self.delay.lock().unwrap() = delay;
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
        thread::sleep(*self.delay.lock().unwrap());
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

#[derive(Default)]
struct GatedParser {
    entered: (Mutex<bool>, Condvar),
    release: (Mutex<bool>, Condvar),
    active: AtomicUsize,
}

impl GatedParser {
    fn wait_until_entered(&self) {
        let (entered, ready) = &self.entered;
        let entered = entered.lock().unwrap();
        let (entered, timeout) = ready
            .wait_timeout_while(entered, Duration::from_secs(2), |entered| !*entered)
            .unwrap();
        assert!(*entered, "parser was never invoked");
        assert!(!timeout.timed_out());
    }

    fn release(&self) {
        let (release, ready) = &self.release;
        *release.lock().unwrap() = true;
        ready.notify_all();
    }
}

impl DocumentParser for GatedParser {
    fn parse(&self, path: &Path, _max_bytes: u64) -> Result<ParsedDocument, ParserError> {
        self.active.fetch_add(1, Ordering::AcqRel);
        let (entered, entered_ready) = &self.entered;
        *entered.lock().unwrap() = true;
        entered_ready.notify_all();
        let (release, release_ready) = &self.release;
        let mut released = release.lock().unwrap();
        while !*released {
            released = release_ready.wait(released).unwrap();
        }
        let body = fs::read_to_string(path).unwrap();
        self.active.fetch_sub(1, Ordering::AcqRel);
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

    fn with_delay(self, delay: Duration) -> Self {
        self.parser.set_delay(delay);
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

    async fn crash_and_recover(&self, job: &str) {
        let old = self.coordinator.read().unwrap().clone();
        old.shutdown_local(job).await;
        let replacement = Arc::new(IndexCoordinator::with_parser(
            Arc::clone(&self.database),
            self.parser.clone(),
            10 * 1024 * 1024,
        ));
        replacement.recover().await.unwrap();
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

    async fn wait_until_total_files(&self, job: &str, minimum: u64) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = self.status(job).await;
            if status.total_files >= minimum {
                return;
            }
            assert!(Instant::now() < deadline, "timed out at {status:?}");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
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

#[derive(Default)]
struct BlockingDiscoveryProbe {
    entered: (Mutex<bool>, Condvar),
    release: (Mutex<bool>, Condvar),
    max_buffered: AtomicUsize,
}

impl BlockingDiscoveryProbe {
    fn wait_until_entered(&self) {
        let (lock, ready) = &self.entered;
        let mut entered = lock.lock().unwrap();
        while !*entered {
            entered = ready.wait(entered).unwrap();
        }
    }

    fn release(&self) {
        let (lock, ready) = &self.release;
        *lock.lock().unwrap() = true;
        ready.notify_all();
    }

    fn wait_until_entered_for(&self, timeout: Duration) -> bool {
        let (lock, ready) = &self.entered;
        let entered = lock.lock().unwrap();
        let (entered, _) = ready
            .wait_timeout_while(entered, timeout, |entered| !*entered)
            .unwrap();
        *entered
    }
}

impl DiscoveryProbe for BlockingDiscoveryProbe {
    fn candidate_persisted(&self, buffered_candidates: usize) {
        self.max_buffered
            .fetch_max(buffered_candidates, Ordering::AcqRel);
        let (entered_lock, entered_ready) = &self.entered;
        *entered_lock.lock().unwrap() = true;
        entered_ready.notify_all();
        let (release_lock, release_ready) = &self.release;
        let mut released = release_lock.lock().unwrap();
        while !*released {
            released = release_ready.wait(released).unwrap();
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReconciliationBarrier {
    CandidateMutation,
    ParserStart,
    StaleDelete,
    CancellationOnly,
}

struct BlockingReconciliationProbe {
    stage: ReconciliationBarrier,
    entered: (Mutex<bool>, Condvar),
    release: (Mutex<bool>, Condvar),
    cancelled: (Mutex<bool>, Condvar),
}

impl BlockingReconciliationProbe {
    fn new(stage: ReconciliationBarrier) -> Self {
        Self {
            stage,
            entered: (Mutex::new(false), Condvar::new()),
            release: (Mutex::new(false), Condvar::new()),
            cancelled: (Mutex::new(false), Condvar::new()),
        }
    }

    fn block_at(&self, stage: ReconciliationBarrier) {
        if self.stage != stage {
            return;
        }
        let (entered, entered_ready) = &self.entered;
        *entered.lock().unwrap() = true;
        entered_ready.notify_all();
        let (release, release_ready) = &self.release;
        let mut released = release.lock().unwrap();
        while !*released {
            released = release_ready.wait(released).unwrap();
        }
    }

    fn wait_until_entered(&self) {
        let (entered, ready) = &self.entered;
        let entered = entered.lock().unwrap();
        let (entered, timeout) = ready
            .wait_timeout_while(entered, Duration::from_secs(2), |entered| !*entered)
            .unwrap();
        assert!(
            *entered,
            "reconciliation never reached the requested barrier"
        );
        assert!(!timeout.timed_out());
    }

    fn wait_until_cancelled(&self) {
        let (cancelled, ready) = &self.cancelled;
        let cancelled = cancelled.lock().unwrap();
        let (cancelled, timeout) = ready
            .wait_timeout_while(cancelled, Duration::from_secs(2), |cancelled| !*cancelled)
            .unwrap();
        assert!(*cancelled, "reconciliation never published cancellation");
        assert!(!timeout.timed_out());
    }

    fn release(&self) {
        let (release, ready) = &self.release;
        *release.lock().unwrap() = true;
        ready.notify_all();
    }
}

impl DiscoveryProbe for BlockingReconciliationProbe {
    fn candidate_persisted(&self, _buffered_candidates: usize) {}

    fn before_reconciliation_candidate_mutation(&self) {
        self.block_at(ReconciliationBarrier::CandidateMutation);
    }

    fn before_reconciliation_parser_start(&self) {
        self.block_at(ReconciliationBarrier::ParserStart);
    }

    fn before_reconciliation_stale_delete(&self) {
        self.block_at(ReconciliationBarrier::StaleDelete);
    }

    fn reconciliation_cancelled(&self) {
        let (cancelled, ready) = &self.cancelled;
        *cancelled.lock().unwrap() = true;
        ready.notify_all();
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
async fn watcher_correlates_separate_rename_halves_and_updates_fts_identity() {
    let harness = IndexHarness::new().with_valid_file("old.txt");
    harness.finish_initial_index().await;
    let old = harness.root.join("old.txt");
    let new = harness.root.join("new.txt");
    fs::rename(&old, &new).unwrap();
    let watcher = harness.watcher.lock().unwrap().as_ref().unwrap().clone();
    watcher
        .ingest(WatchChange::RenameFrom {
            path: old,
            tracker: Some(77),
        })
        .await
        .unwrap();
    watcher
        .ingest(WatchChange::RenameTo {
            path: new,
            tracker: Some(77),
        })
        .await
        .unwrap();
    watcher.flush().await.unwrap();

    assert!(harness.document("old.txt").await.is_none());
    assert!(harness.document("new.txt").await.is_some());
    assert_eq!(harness.search("new").await.len(), 1);
}

#[tokio::test]
async fn watcher_does_not_drop_a_legitimate_write_after_a_completed_batch() {
    let harness = IndexHarness::new().with_valid_file("later.txt");
    harness.finish_initial_index().await;
    harness.emit_repeated_write_events("later.txt", 3).await;
    assert_eq!(harness.parse_count("later.txt"), 2);

    harness.write("later.txt", "a genuinely later and different body");
    let watcher = harness.watcher.lock().unwrap().as_ref().unwrap().clone();
    watcher
        .ingest(WatchChange::Write(harness.root.join("later.txt")))
        .await
        .unwrap();
    watcher.flush().await.unwrap();

    assert_eq!(harness.parse_count("later.txt"), 3);
}

#[tokio::test]
async fn startup_restores_enabled_watchers_and_drop_stops_observation() {
    let harness = IndexHarness::new().with_valid_file("restored.txt");
    let job = harness.start().await;
    assert_eq!(
        harness.wait_until_finished(&job).await.state,
        JobState::Completed
    );
    let state = AppState::new(
        Arc::clone(&harness.database),
        harness.coordinator.read().unwrap().clone(),
    );
    state.restore_runtime().await.unwrap();
    assert_eq!(state.watchers.lock().await.len(), 1);

    harness.write("restored.txt", "observed after restart");
    let deadline = Instant::now() + Duration::from_secs(5);
    while harness.parse_count("restored.txt") < 2 {
        assert!(Instant::now() < deadline, "restored watcher did not index");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    drop(state);
    harness.write("restored.txt", "must not be observed after shutdown");
    tokio::time::sleep(Duration::from_millis(750)).await;
    assert_eq!(harness.parse_count("restored.txt"), 2);
}

#[tokio::test]
async fn failed_watcher_activation_rolls_back_folder_registration() {
    let harness = IndexHarness::new();
    let state = AppState::new(
        Arc::clone(&harness.database),
        harness.coordinator.read().unwrap().clone(),
    );
    fs::remove_dir(&harness.root).unwrap();

    assert!(state
        .activate_registered_folder(&harness.folder_id)
        .await
        .is_err());
    assert!(state.folders.list().unwrap().is_empty());
    assert!(state.watchers.lock().await.is_empty());
}

#[tokio::test]
async fn reconciliation_streams_without_blocking_and_cleans_its_seen_set() {
    let harness = IndexHarness::new().with_files([
        "r00.txt", "r01.txt", "r02.txt", "r03.txt", "r04.txt", "r05.txt", "r06.txt", "r07.txt",
        "r08.txt", "r09.txt", "r10.txt", "r11.txt", "r12.txt", "r13.txt", "r14.txt", "r15.txt",
        "r16.txt", "r17.txt", "r18.txt", "r19.txt",
    ]);
    let job = harness.start().await;
    assert_eq!(
        harness.wait_until_finished(&job).await.state,
        JobState::Completed
    );
    for index in 0..5 {
        fs::remove_file(harness.root.join(format!("r{index:02}.txt"))).unwrap();
    }
    harness.write("r10.txt", "changed during reconciliation");
    harness.write("added.txt", "added");

    let coordinator = harness.coordinator.read().unwrap().clone();
    let folder_id = harness.folder_id.clone();
    let reconciliation = tokio::spawn(async move { coordinator.reconcile(&folder_id).await });
    tokio::time::timeout(Duration::from_millis(100), tokio::task::yield_now())
        .await
        .expect("reconciliation blocked the async runtime");
    reconciliation.await.unwrap().unwrap();

    assert!(harness.document("r00.txt").await.is_none());
    assert!(harness.document("r10.txt").await.is_some());
    assert!(harness.document("added.txt").await.is_some());
    let seen: i64 = harness
        .database
        .connection()
        .query_row("SELECT COUNT(*) FROM reconciliation_seen", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(seen, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn reconciliation_waiting_for_database_does_not_block_single_thread_runtime() {
    let harness = IndexHarness::new().with_valid_file("sentinel.txt");
    let job = harness.start().await;
    assert_eq!(
        harness.wait_until_finished(&job).await.state,
        JobState::Completed
    );
    let database = Arc::clone(&harness.database);
    let (locked_sender, locked_receiver) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        let _guard = database.connection();
        locked_sender.send(()).unwrap();
        std::thread::sleep(Duration::from_millis(250));
    });
    locked_receiver.recv().unwrap();

    let coordinator = harness.coordinator.read().unwrap().clone();
    let folder_id = harness.folder_id.clone();
    let started = Instant::now();
    let reconciliation = tokio::spawn(async move { coordinator.reconcile(&folder_id).await });
    tokio::time::sleep(Duration::from_millis(25)).await;
    assert!(
        started.elapsed() < Duration::from_millis(100),
        "database mutex wait blocked the single-thread runtime"
    );
    holder.join().unwrap();
    reconciliation.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aborted_reconciliation_cleans_run_scoped_scratch_rows() {
    let harness = IndexHarness::new().with_files(["a.txt", "b.txt"]);
    let stale_path = harness.root.join("must-remain-after-abort.txt");
    harness
        .database
        .connection()
        .execute(
            "INSERT INTO documents
             (id, folder_id, canonical_path, file_name, extension, size_bytes,
              modified_at, parse_state)
             VALUES ('stale-doc', ?1, ?2, 'must-remain-after-abort.txt', 'txt',
                     1, '1', 'indexed')",
            rusqlite::params![harness.folder_id, stale_path.to_string_lossy()],
        )
        .unwrap();
    let probe = Arc::new(BlockingDiscoveryProbe::default());
    let coordinator = Arc::new(IndexCoordinator::with_parser_and_discovery_probe(
        Arc::clone(&harness.database),
        Arc::clone(&harness.parser),
        10 * 1024 * 1024,
        Arc::clone(&probe),
    ));
    *harness.coordinator.write().unwrap() = Arc::clone(&coordinator);
    let folder_id = harness.folder_id.clone();
    let reconciliation = tokio::spawn(async move { coordinator.reconcile(&folder_id).await });
    if !probe.wait_until_entered_for(Duration::from_secs(2)) {
        reconciliation.abort();
        probe.release();
        panic!("reconciliation never reached its persisted scratch row");
    }
    let runs: i64 = harness
        .database
        .connection()
        .query_row("SELECT COUNT(*) FROM reconciliation_runs", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(runs, 1);

    reconciliation.abort();
    probe.release();
    let aborted = reconciliation.await.unwrap_err();
    assert!(aborted.is_cancelled());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let counts: (i64, i64) = harness
            .database
            .connection()
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM reconciliation_runs),
                   (SELECT COUNT(*) FROM reconciliation_seen_v2)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        if counts == (0, 0) {
            break;
        }
        assert!(Instant::now() < deadline, "scratch rows leaked: {counts:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(harness
        .document("must-remain-after-abort.txt")
        .await
        .is_some());
    assert!(harness.document("a.txt").await.is_none());
    assert!(harness.document("b.txt").await.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abort_before_candidate_mutation_writes_no_scratch_job_or_document() {
    let harness = IndexHarness::new().with_valid_file("after-cancel.txt");
    harness
        .database
        .connection()
        .execute_batch(
            "CREATE TABLE reconciliation_mutation_audit (
               canonical_path TEXT NOT NULL
             );
             CREATE TRIGGER audit_reconciliation_seen
             AFTER INSERT ON reconciliation_seen_v2
             BEGIN
               INSERT INTO reconciliation_mutation_audit (canonical_path)
               VALUES (NEW.canonical_path);
             END;",
        )
        .unwrap();
    let probe = Arc::new(BlockingReconciliationProbe::new(
        ReconciliationBarrier::CandidateMutation,
    ));
    let coordinator = Arc::new(IndexCoordinator::with_parser_and_discovery_probe(
        Arc::clone(&harness.database),
        Arc::clone(&harness.parser),
        10 * 1024 * 1024,
        Arc::clone(&probe),
    ));
    let folder_id = harness.folder_id.clone();
    let reconciliation = tokio::spawn(async move { coordinator.reconcile(&folder_id).await });
    probe.wait_until_entered();

    reconciliation.abort();
    probe.wait_until_cancelled();
    probe.release();
    assert!(reconciliation.await.unwrap_err().is_cancelled());

    let (scratch_writes, jobs) = {
        let connection = harness.database.connection();
        let scratch_writes: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM reconciliation_mutation_audit",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let jobs: i64 = connection
            .query_row("SELECT COUNT(*) FROM index_jobs", [], |row| row.get(0))
            .unwrap();
        (scratch_writes, jobs)
    };
    assert_eq!(scratch_writes, 0);
    assert_eq!(jobs, 0);
    assert_eq!(harness.parse_count("after-cancel.txt"), 0);
    assert!(harness.document("after-cancel.txt").await.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abort_before_parser_start_never_marks_or_invokes_the_parser() {
    let harness = IndexHarness::new().with_valid_file("after-cancel.txt");
    let probe = Arc::new(BlockingReconciliationProbe::new(
        ReconciliationBarrier::ParserStart,
    ));
    let coordinator = Arc::new(IndexCoordinator::with_parser_and_discovery_probe(
        Arc::clone(&harness.database),
        Arc::clone(&harness.parser),
        10 * 1024 * 1024,
        Arc::clone(&probe),
    ));
    let folder_id = harness.folder_id.clone();
    let reconciliation = tokio::spawn(async move { coordinator.reconcile(&folder_id).await });
    probe.wait_until_entered();

    reconciliation.abort();
    probe.wait_until_cancelled();
    probe.release();
    assert!(reconciliation.await.unwrap_err().is_cancelled());

    assert_eq!(harness.parse_count("after-cancel.txt"), 0);
    assert!(harness.document("after-cancel.txt").await.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abort_during_an_active_parser_waits_and_blocks_the_final_commit() {
    let harness = IndexHarness::new().with_valid_file("in-flight.txt");
    let parser = Arc::new(GatedParser::default());
    let probe = Arc::new(BlockingReconciliationProbe::new(
        ReconciliationBarrier::CancellationOnly,
    ));
    let coordinator = Arc::new(IndexCoordinator::with_parser_and_discovery_probe(
        Arc::clone(&harness.database),
        Arc::clone(&parser),
        10 * 1024 * 1024,
        Arc::clone(&probe),
    ));
    let folder_id = harness.folder_id.clone();
    let reconciliation_coordinator = Arc::clone(&coordinator);
    let reconciliation =
        tokio::spawn(async move { reconciliation_coordinator.reconcile(&folder_id).await });
    parser.wait_until_entered();

    reconciliation.abort();
    probe.wait_until_cancelled();
    assert_eq!(parser.active.load(Ordering::Acquire), 1);
    assert!(
        !reconciliation.is_finished(),
        "reconciliation abort returned while its parser was active"
    );

    parser.release();
    assert!(reconciliation.await.unwrap_err().is_cancelled());
    assert_eq!(parser.active.load(Ordering::Acquire), 0);
    let cancelled_state: (i64, i64, i64) = harness
        .database
        .connection()
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM documents
                WHERE folder_id = ?1 AND file_name = 'in-flight.txt'
                  AND parse_state = 'parsing'),
               (SELECT COUNT(*) FROM document_content
                WHERE document_id IN (
                  SELECT id FROM documents
                  WHERE folder_id = ?1 AND file_name = 'in-flight.txt'
                )),
               (SELECT COUNT(*) FROM document_fts
                WHERE document_id IN (
                  SELECT id FROM documents
                  WHERE folder_id = ?1 AND file_name = 'in-flight.txt'
                ))",
            [&harness.folder_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(cancelled_state, (0, 0, 0));

    coordinator.reconcile(&harness.folder_id).await.unwrap();
    let recovered: (String, String, String) = harness
        .database
        .connection()
        .query_row(
            "SELECT documents.parse_state, document_content.body, document_fts.body
             FROM documents
             JOIN document_content ON document_content.document_id = documents.id
             JOIN document_fts ON document_fts.document_id = documents.id
             WHERE documents.folder_id = ?1
               AND documents.file_name = 'in-flight.txt'",
            [&harness.folder_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        recovered,
        (
            "parsed".to_owned(),
            "in-flight".to_owned(),
            "in-flight".to_owned()
        )
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abort_during_an_active_parser_restores_existing_content_for_retry() {
    let harness = IndexHarness::new().with_valid_file("existing.txt");
    let initial_job = harness.start().await;
    assert_eq!(
        harness.wait_until_finished(&initial_job).await.state,
        JobState::Completed
    );
    let before: (i64, String, String, Option<String>, String, String) = harness
        .database
        .connection()
        .query_row(
            "SELECT documents.size_bytes, documents.modified_at,
                    documents.parse_state, documents.parse_error_code,
                    document_content.body, document_fts.body
             FROM documents
             JOIN document_content ON document_content.document_id = documents.id
             JOIN document_fts ON document_fts.document_id = documents.id
             WHERE documents.folder_id = ?1
               AND documents.file_name = 'existing.txt'",
            [&harness.folder_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(before.2, "parsed");
    assert_eq!(before.4, "existing");
    assert_eq!(before.5, "existing");

    harness.write("existing.txt", "replacement body indexed after retry");
    let parser = Arc::new(GatedParser::default());
    let probe = Arc::new(BlockingReconciliationProbe::new(
        ReconciliationBarrier::CancellationOnly,
    ));
    let coordinator = Arc::new(IndexCoordinator::with_parser_and_discovery_probe(
        Arc::clone(&harness.database),
        Arc::clone(&parser),
        10 * 1024 * 1024,
        Arc::clone(&probe),
    ));
    let folder_id = harness.folder_id.clone();
    let reconciliation_coordinator = Arc::clone(&coordinator);
    let reconciliation =
        tokio::spawn(async move { reconciliation_coordinator.reconcile(&folder_id).await });
    parser.wait_until_entered();

    reconciliation.abort();
    probe.wait_until_cancelled();
    assert_eq!(parser.active.load(Ordering::Acquire), 1);
    assert!(
        !reconciliation.is_finished(),
        "reconciliation abort returned while its parser was active"
    );

    parser.release();
    assert!(reconciliation.await.unwrap_err().is_cancelled());
    assert_eq!(parser.active.load(Ordering::Acquire), 0);
    let after_cancel: (i64, String, String, Option<String>, String, String) = harness
        .database
        .connection()
        .query_row(
            "SELECT documents.size_bytes, documents.modified_at,
                    documents.parse_state, documents.parse_error_code,
                    document_content.body, document_fts.body
             FROM documents
             JOIN document_content ON document_content.document_id = documents.id
             JOIN document_fts ON document_fts.document_id = documents.id
             WHERE documents.folder_id = ?1
               AND documents.file_name = 'existing.txt'",
            [&harness.folder_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(after_cancel, before);

    coordinator.reconcile(&harness.folder_id).await.unwrap();
    let recovered: (String, String, String) = harness
        .database
        .connection()
        .query_row(
            "SELECT documents.parse_state, document_content.body, document_fts.body
             FROM documents
             JOIN document_content ON document_content.document_id = documents.id
             JOIN document_fts ON document_fts.document_id = documents.id
             WHERE documents.folder_id = ?1
               AND documents.file_name = 'existing.txt'",
            [&harness.folder_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        recovered,
        (
            "parsed".to_owned(),
            "replacement body indexed after retry".to_owned(),
            "replacement body indexed after retry".to_owned()
        )
    );
}

#[tokio::test]
async fn reconciliation_retries_matching_nonterminal_and_incomplete_documents() {
    let harness = IndexHarness::new().with_files([
        "parsing.txt",
        "failed.txt",
        "pending.txt",
        "missing-content.txt",
        "missing-fts.txt",
        "ready.txt",
    ]);
    let initial_job = harness.start().await;
    assert_eq!(
        harness.wait_until_finished(&initial_job).await.state,
        JobState::Completed
    );
    harness
        .database
        .connection()
        .execute_batch(
            "UPDATE documents SET parse_state = 'parsing'
             WHERE file_name = 'parsing.txt';
             UPDATE documents SET parse_state = 'failed', parse_error_code = 'DAMAGED'
             WHERE file_name = 'failed.txt';
             UPDATE documents SET parse_state = 'pending'
             WHERE file_name = 'pending.txt';
             DELETE FROM document_content
             WHERE document_id IN (
               SELECT id FROM documents WHERE file_name = 'missing-content.txt'
             );
             DELETE FROM document_fts
             WHERE document_id IN (
               SELECT id FROM documents WHERE file_name = 'missing-fts.txt'
             );",
        )
        .unwrap();

    let coordinator = harness.coordinator.read().unwrap().clone();
    coordinator.reconcile(&harness.folder_id).await.unwrap();

    for name in [
        "parsing.txt",
        "failed.txt",
        "pending.txt",
        "missing-content.txt",
        "missing-fts.txt",
    ] {
        assert_eq!(
            harness.parse_count(name),
            2,
            "{name} was incorrectly accepted as content-ready"
        );
    }
    assert_eq!(harness.parse_count("ready.txt"), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abort_after_stale_selection_never_deletes_the_document() {
    let harness = IndexHarness::new();
    let stale_path = harness.root.join("must-survive-cancel.txt");
    harness
        .database
        .connection()
        .execute(
            "INSERT INTO documents
             (id, folder_id, canonical_path, file_name, extension, size_bytes,
              modified_at, parse_state)
             VALUES ('stale-doc', ?1, ?2, 'must-survive-cancel.txt', 'txt',
                     1, '1', 'indexed')",
            rusqlite::params![harness.folder_id, stale_path.to_string_lossy()],
        )
        .unwrap();
    let probe = Arc::new(BlockingReconciliationProbe::new(
        ReconciliationBarrier::StaleDelete,
    ));
    let coordinator = Arc::new(IndexCoordinator::with_parser_and_discovery_probe(
        Arc::clone(&harness.database),
        Arc::clone(&harness.parser),
        10 * 1024 * 1024,
        Arc::clone(&probe),
    ));
    let folder_id = harness.folder_id.clone();
    let reconciliation = tokio::spawn(async move { coordinator.reconcile(&folder_id).await });
    probe.wait_until_entered();

    reconciliation.abort();
    probe.wait_until_cancelled();
    probe.release();
    assert!(reconciliation.await.unwrap_err().is_cancelled());

    assert!(harness.document("must-survive-cancel.txt").await.is_some());
}

#[tokio::test]
async fn watcher_overflow_records_a_typed_diagnostic_and_reconciles() {
    let harness = IndexHarness::new().with_valid_file("overflow.txt");
    harness.finish_initial_index().await;
    harness.write("overflow.txt", "changed after overflow");
    let watcher = harness.watcher.lock().unwrap().as_ref().unwrap().clone();
    let mut overflowed = false;
    for _ in 0..2_000 {
        if watcher
            .ingest_nowait(WatchChange::Write(harness.root.join("overflow.txt")))
            .is_err()
        {
            overflowed = true;
        }
    }
    assert!(
        overflowed,
        "test did not saturate the bounded watcher channel"
    );
    watcher.flush().await.unwrap();

    let diagnostics: i64 = harness
        .database
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM index_job_errors WHERE code = 'WATCHER_OVERFLOW'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(diagnostics >= 1);
    assert_eq!(harness.parse_count("overflow.txt"), 2);
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

#[tokio::test]
async fn crashed_active_job_is_recovered_without_reparsing_committed_files() {
    let harness = IndexHarness::new()
        .with_files(["a.txt", "b.txt", "c.txt"])
        .with_delay(Duration::from_millis(40));
    let job = harness.start().await;
    harness.wait_until_completed_files(&job, 1).await;

    harness.crash_and_recover(&job).await;

    let status = harness.wait_until_finished(&job).await;
    assert_eq!(status.completed_files, 3);
    assert_eq!(harness.parse_count("a.txt"), 1);
}

#[tokio::test]
async fn pause_resume_and_cancel_are_quiescent_and_single_owner() {
    let harness = IndexHarness::new().with_delay(Duration::from_millis(35));
    let job = harness.start_with_many_files(40).await;
    harness.wait_until_total_files(&job, 33).await;
    let coordinator = harness.coordinator.read().unwrap().clone();

    tokio::time::timeout(Duration::from_secs(3), coordinator.pause(&job))
        .await
        .unwrap()
        .unwrap();
    let paused_progress = harness.status(&job).await.completed_files;
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(harness.status(&job).await.completed_files, paused_progress);

    let left = {
        let coordinator = Arc::clone(&coordinator);
        let job = job.clone();
        tokio::spawn(async move { coordinator.resume(&job).await })
    };
    let right = {
        let coordinator = Arc::clone(&coordinator);
        let job = job.clone();
        tokio::spawn(async move { coordinator.resume(&job).await })
    };
    let results = [left.await.unwrap(), right.await.unwrap()];
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);

    tokio::time::timeout(Duration::from_secs(3), coordinator.cancel(&job))
        .await
        .unwrap()
        .unwrap();
    let cancelled_progress = harness.status(&job).await.completed_files;
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(
        harness.status(&job).await.completed_files,
        cancelled_progress
    );
    assert_eq!(harness.status(&job).await.state, JobState::Cancelled);
    for index in 0..40 {
        assert!(harness.parse_count(&format!("bulk-{index:03}.txt")) <= 1);
    }
}

#[tokio::test]
async fn failed_document_and_progress_roll_back_together() {
    let harness = IndexHarness::new().with_damaged_file("broken.pdf");
    harness
        .database
        .connection()
        .execute_batch(
            "CREATE TRIGGER abort_index_error
             BEFORE INSERT ON index_job_errors
             BEGIN
               SELECT RAISE(ABORT, 'injected crash boundary');
             END;",
        )
        .unwrap();

    let job = harness.start().await;
    let status = harness.wait_until_finished(&job).await;
    let connection = harness.database.connection();
    let parse_state: String = connection
        .query_row(
            "SELECT parse_state FROM documents WHERE file_name = 'broken.pdf'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let completed_file_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM index_job_files
             WHERE job_id = ?1 AND state = 'completed'",
            [&job],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(status.state, JobState::Failed);
    assert_eq!(status.completed_files, 0);
    assert!(status.errors.is_empty());
    assert_eq!(parse_state, "parsing");
    assert_eq!(completed_file_count, 0);
}

#[tokio::test]
async fn start_returns_before_incremental_discovery_finishes_and_stays_bounded() {
    let harness = IndexHarness::new().with_files(["a.txt", "b.txt", "c.txt"]);
    let probe = Arc::new(BlockingDiscoveryProbe::default());
    let coordinator = Arc::new(IndexCoordinator::with_parser_and_discovery_probe(
        Arc::clone(&harness.database),
        harness.parser.clone(),
        10 * 1024 * 1024,
        probe.clone(),
    ));
    *harness.coordinator.write().unwrap() = Arc::clone(&coordinator);

    let job = tokio::time::timeout(
        Duration::from_millis(100),
        coordinator.start(&harness.folder_id),
    )
    .await
    .expect("start waited for discovery")
    .unwrap();
    tokio::task::spawn_blocking({
        let probe = Arc::clone(&probe);
        move || probe.wait_until_entered()
    })
    .await
    .unwrap();
    let discovering = coordinator.status(&job).await.unwrap();
    assert_eq!(discovering.state, JobState::Discovering);
    assert_eq!(discovering.total_files, 1);
    assert!(probe.max_buffered.load(Ordering::Acquire) <= 16);

    probe.release();
    assert_eq!(
        harness.wait_until_finished(&job).await.state,
        JobState::Completed
    );
}
