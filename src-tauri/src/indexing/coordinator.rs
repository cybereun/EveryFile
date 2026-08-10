use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock, Weak};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rand::RngExt;
use rusqlite::{params, OptionalExtension};
use thiserror::Error;
use tokio::sync::{mpsc, Notify};
use tokio::task::{JoinHandle, JoinSet};

use crate::domain::models::{AppSettings, FolderRecord, IndexFailure, IndexStatus, JobState};
use crate::folders::discovery::{discover, DiscoveryOptions, DiscoveryPoll, FileCandidate};
use crate::infrastructure::database::Database;
use crate::ocr::eligibility::{decide as decide_ocr, OcrDecision};
use crate::ocr::{OcrClient, OcrError, OcrMode};
use crate::parsing::{ParseErrorCode, ParsedDocument, ParserClient, ParserError};

const PIPELINE_CAPACITY: usize = 16;
const DISCOVERY_PERSIST_BATCH_SIZE: usize = 32;
const MAX_PARSER_CONCURRENCY: usize = 3;
const MAX_TERMINAL_ERROR_DETAILS: i64 = 100;
const MAX_PARSE_ATTEMPT_TOKEN_CLAIMS: usize = 4;

pub type JobId = String;

impl JobState {
    fn as_sql(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Discovering => "discovering",
            Self::Parsing => "parsing",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    fn from_sql(value: &str) -> Result<Self, IndexingError> {
        match value {
            "queued" => Ok(Self::Queued),
            "discovering" => Ok(Self::Discovering),
            "parsing" => Ok(Self::Parsing),
            "paused" => Ok(Self::Paused),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            "failed" => Ok(Self::Failed),
            value => Err(IndexingError::InvalidState(value.to_owned())),
        }
    }
}

pub trait DocumentParser: Send + Sync + 'static {
    fn parse(&self, path: &Path, max_bytes: u64) -> Result<ParsedDocument, ParserError>;
}

pub trait DocumentOcr: Send + Sync + 'static {
    fn recognize(
        &self,
        path: &Path,
        mode: OcrMode,
        max_bytes: u64,
        cancelled: Arc<AtomicBool>,
    ) -> Result<ParsedDocument, OcrError>;
}

pub trait ParseAttemptTokenGenerator: Send + Sync + 'static {
    fn generate(&self) -> String;
}

pub trait DiscoveryProbe: Send + Sync + 'static {
    fn candidate_persisted(&self, buffered_candidates: usize);

    fn before_reconciliation_candidate_mutation(&self) {}

    fn before_reconciliation_parser_start(&self) {}

    fn before_reconciliation_stale_delete(&self) {}

    fn reconciliation_cancelled(&self) {}
}

struct NoopDiscoveryProbe;
struct SecureParseAttemptTokenGenerator;

impl DiscoveryProbe for NoopDiscoveryProbe {
    fn candidate_persisted(&self, _buffered_candidates: usize) {}
}

impl ParseAttemptTokenGenerator for SecureParseAttemptTokenGenerator {
    fn generate(&self) -> String {
        random_id()
    }
}

impl DocumentParser for ParserClient {
    fn parse(&self, path: &Path, max_bytes: u64) -> Result<ParsedDocument, ParserError> {
        ParserClient::parse(self, path, max_bytes)
    }
}

impl DocumentOcr for OcrClient {
    fn recognize(
        &self,
        path: &Path,
        mode: OcrMode,
        max_bytes: u64,
        cancelled: Arc<AtomicBool>,
    ) -> Result<ParsedDocument, OcrError> {
        OcrClient::recognize(self, path, mode, max_bytes, cancelled)
    }
}

#[derive(Clone, Default)]
pub struct ActivityLimiter {
    inner: Arc<ActivityLimiterInner>,
}

#[derive(Default)]
struct ActivityLimiterInner {
    foreground_count: AtomicUsize,
    idle: Notify,
}

impl ActivityLimiter {
    pub fn begin_foreground(&self) -> ForegroundActivity {
        self.inner.foreground_count.fetch_add(1, Ordering::AcqRel);
        ForegroundActivity {
            limiter: self.clone(),
        }
    }

    async fn yield_to_foreground(&self) {
        loop {
            let notified = self.inner.idle.notified();
            if self.inner.foreground_count.load(Ordering::Acquire) == 0 {
                break;
            }
            notified.await;
        }
        tokio::task::yield_now().await;
    }
}

pub struct ForegroundActivity {
    limiter: ActivityLimiter,
}

impl Drop for ForegroundActivity {
    fn drop(&mut self) {
        if self
            .limiter
            .inner
            .foreground_count
            .fetch_sub(1, Ordering::AcqRel)
            == 1
        {
            self.limiter.inner.idle.notify_waiters();
        }
    }
}

type StatusSink = dyn Fn(IndexStatus) -> Result<(), String> + Send + Sync;

#[derive(Clone)]
pub struct IndexCoordinator {
    database: Arc<Database>,
    parser: Arc<dyn DocumentParser>,
    ocr: Option<Arc<dyn DocumentOcr>>,
    runtime_settings: Arc<RwLock<RuntimeIndexSettings>>,
    limiter: ActivityLimiter,
    runtimes: Arc<tokio::sync::Mutex<HashMap<JobId, Arc<JobRuntime>>>>,
    status_sink: Option<Arc<StatusSink>>,
    discovery_probe: Arc<dyn DiscoveryProbe>,
    attempt_tokens: Arc<dyn ParseAttemptTokenGenerator>,
}

#[derive(Clone)]
struct RuntimeIndexSettings {
    max_file_size_bytes: u64,
    excluded_path_patterns: Vec<String>,
    indexing_intensity: String,
    ocr_enabled: bool,
    math_ocr_enabled: bool,
}

enum ExtractionError {
    Parser(ParserError),
    Ocr(OcrError),
    OcrUnavailable,
}

struct JobRuntime {
    stop: Arc<AtomicBool>,
    ownership: tokio::sync::Mutex<()>,
    gate: tokio::sync::Mutex<()>,
    commit_gate: parking_lot::Mutex<()>,
    reconciliation_gate: Option<Arc<ReconciliationMutationGate>>,
    handle: tokio::sync::Mutex<Option<JoinHandle<()>>>,
}

impl JobRuntime {
    fn new() -> Self {
        Self::with_reconciliation_gate(None)
    }

    fn for_reconciliation(gate: Arc<ReconciliationMutationGate>) -> Self {
        Self::with_reconciliation_gate(Some(gate))
    }

    fn with_reconciliation_gate(
        reconciliation_gate: Option<Arc<ReconciliationMutationGate>>,
    ) -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            ownership: tokio::sync::Mutex::new(()),
            gate: tokio::sync::Mutex::new(()),
            commit_gate: parking_lot::Mutex::new(()),
            reconciliation_gate,
            handle: tokio::sync::Mutex::new(None),
        }
    }

    fn with_mutation<T>(
        &self,
        mutation: impl FnOnce() -> Result<T, IndexingError>,
    ) -> Result<T, IndexingError> {
        if let Some(gate) = &self.reconciliation_gate {
            gate.with_mutation(mutation)
        } else {
            let _commit = self.commit_gate.lock();
            if self.stop.load(Ordering::Acquire) {
                return Err(IndexingError::StateChanged);
            }
            mutation()
        }
    }

    fn begin_parser_operation<T>(
        &self,
        mutation: impl FnOnce() -> Result<T, IndexingError>,
    ) -> Result<(ParserOperationLease, T), IndexingError> {
        if let Some(gate) = &self.reconciliation_gate {
            let (operation, value) = gate.begin_operation(mutation)?;
            Ok((
                ParserOperationLease {
                    reconciliation: Some(operation),
                },
                value,
            ))
        } else {
            let _commit = self.commit_gate.lock();
            if self.stop.load(Ordering::Acquire) {
                return Err(IndexingError::StateChanged);
            }
            let value = mutation()?;
            Ok((
                ParserOperationLease {
                    reconciliation: None,
                },
                value,
            ))
        }
    }

    fn finish_parser_operation<T>(
        &self,
        operation: ParserOperationLease,
        commit: impl FnOnce() -> Result<T, IndexingError>,
        compensate: impl FnOnce() -> Result<(), IndexingError>,
    ) -> Result<T, IndexingError> {
        if let Some(operation) = operation.reconciliation {
            operation.finish(commit, compensate)
        } else {
            let _commit = self.commit_gate.lock();
            if self.stop.load(Ordering::Acquire) {
                compensate()?;
                Err(IndexingError::StateChanged)
            } else {
                commit()
            }
        }
    }
}

struct ParserOperationLease {
    reconciliation: Option<ActiveReconciliationOperation>,
}

impl IndexCoordinator {
    pub fn with_parser<P>(database: Arc<Database>, parser: Arc<P>, max_file_size_bytes: u64) -> Self
    where
        P: DocumentParser,
    {
        Self::with_parser_probe_and_sink(
            database,
            parser,
            None,
            max_file_size_bytes,
            Arc::new(NoopDiscoveryProbe),
            None,
            Arc::new(SecureParseAttemptTokenGenerator),
        )
    }

    pub fn with_parser_and_sink<P>(
        database: Arc<Database>,
        parser: Arc<P>,
        max_file_size_bytes: u64,
        status_sink: Option<Arc<StatusSink>>,
    ) -> Self
    where
        P: DocumentParser,
    {
        Self::with_parser_probe_and_sink(
            database,
            parser,
            None,
            max_file_size_bytes,
            Arc::new(NoopDiscoveryProbe),
            status_sink,
            Arc::new(SecureParseAttemptTokenGenerator),
        )
    }

    pub fn with_parser_ocr_and_sink<P, O>(
        database: Arc<Database>,
        parser: Arc<P>,
        ocr: Arc<O>,
        max_file_size_bytes: u64,
        status_sink: Option<Arc<StatusSink>>,
    ) -> Self
    where
        P: DocumentParser,
        O: DocumentOcr,
    {
        Self::with_parser_probe_and_sink(
            database,
            parser,
            Some(ocr),
            max_file_size_bytes,
            Arc::new(NoopDiscoveryProbe),
            status_sink,
            Arc::new(SecureParseAttemptTokenGenerator),
        )
    }

    pub fn with_parser_and_discovery_probe<P, D>(
        database: Arc<Database>,
        parser: Arc<P>,
        max_file_size_bytes: u64,
        discovery_probe: Arc<D>,
    ) -> Self
    where
        P: DocumentParser,
        D: DiscoveryProbe,
    {
        Self::with_parser_probe_and_sink(
            database,
            parser,
            None,
            max_file_size_bytes,
            discovery_probe,
            None,
            Arc::new(SecureParseAttemptTokenGenerator),
        )
    }

    pub fn with_parser_and_token_generator<P, T>(
        database: Arc<Database>,
        parser: Arc<P>,
        max_file_size_bytes: u64,
        attempt_tokens: Arc<T>,
    ) -> Self
    where
        P: DocumentParser,
        T: ParseAttemptTokenGenerator,
    {
        Self::with_parser_probe_and_sink(
            database,
            parser,
            None,
            max_file_size_bytes,
            Arc::new(NoopDiscoveryProbe),
            None,
            attempt_tokens,
        )
    }

    fn with_parser_probe_and_sink<P>(
        database: Arc<Database>,
        parser: Arc<P>,
        ocr: Option<Arc<dyn DocumentOcr>>,
        max_file_size_bytes: u64,
        discovery_probe: Arc<dyn DiscoveryProbe>,
        status_sink: Option<Arc<StatusSink>>,
        attempt_tokens: Arc<dyn ParseAttemptTokenGenerator>,
    ) -> Self
    where
        P: DocumentParser,
    {
        Self {
            database,
            parser,
            ocr,
            runtime_settings: Arc::new(RwLock::new(RuntimeIndexSettings {
                max_file_size_bytes,
                excluded_path_patterns: Vec::new(),
                indexing_intensity: "balanced".into(),
                ocr_enabled: false,
                math_ocr_enabled: false,
            })),
            limiter: ActivityLimiter::default(),
            runtimes: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            status_sink,
            discovery_probe,
            attempt_tokens,
        }
    }

    pub fn activity_limiter(&self) -> ActivityLimiter {
        self.limiter.clone()
    }

    pub fn apply_runtime_settings(&self, settings: &AppSettings) -> Result<(), IndexingError> {
        let replacement = RuntimeIndexSettings {
            max_file_size_bytes: settings.max_file_size_bytes,
            excluded_path_patterns: settings.excluded_path_patterns.clone(),
            indexing_intensity: settings.indexing_intensity.clone(),
            ocr_enabled: settings.ocr_enabled,
            math_ocr_enabled: settings.math_ocr_enabled,
        };
        *self
            .runtime_settings
            .write()
            .map_err(|_| IndexingError::RuntimeSettingsUnavailable)? = replacement;
        Ok(())
    }

    fn discovery_options(&self) -> Result<DiscoveryOptions, IndexingError> {
        let settings = self
            .runtime_settings
            .read()
            .map_err(|_| IndexingError::RuntimeSettingsUnavailable)?;
        Ok(DiscoveryOptions::default()
            .with_excluded_path_patterns(settings.excluded_path_patterns.clone()))
    }

    async fn apply_intensity_delay(&self) -> Result<(), IndexingError> {
        let intensity = self
            .runtime_settings
            .read()
            .map_err(|_| IndexingError::RuntimeSettingsUnavailable)?
            .indexing_intensity
            .clone();
        match intensity.as_str() {
            "low" => tokio::time::sleep(Duration::from_millis(20)).await,
            "balanced" => tokio::task::yield_now().await,
            "high" => {}
            _ => return Err(IndexingError::RuntimeSettingsUnavailable),
        }
        Ok(())
    }

    pub async fn start(&self, folder_id: &str) -> Result<JobId, IndexingError> {
        self.registered_folder(folder_id)?;
        let job_id = random_id();
        {
            let mut connection = self.database.connection();
            let transaction = connection.transaction()?;
            transaction.execute(
                "INSERT INTO index_jobs
                 (id, folder_id, state, completed_files, total_files, last_path, updated_at, origin)
                 VALUES (?1, ?2, ?3, 0, 0, NULL, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), 'manual')",
                params![job_id, folder_id, JobState::Queued.as_sql()],
            )?;
            transaction.execute(
                "INSERT INTO index_job_recovery (job_id, discovery_complete)
                 VALUES (?1, 0)",
                [&job_id],
            )?;
            transaction.commit()?;
        }
        self.emit_status(&job_id);
        self.spawn_job(job_id.clone()).await?;
        Ok(job_id)
    }

    pub async fn pause(&self, job_id: &str) -> Result<(), IndexingError> {
        let runtime = self.runtime(job_id).await;
        let ownership = runtime.ownership.lock().await;
        let gate = runtime.gate.lock().await;
        self.cas_active_state(job_id, JobState::Paused)?;
        runtime.stop.store(true, Ordering::Release);
        drop(gate);
        self.await_worker(&runtime).await;
        drop(ownership);
        self.emit_status(job_id);
        Ok(())
    }

    pub async fn resume(&self, job_id: &str) -> Result<(), IndexingError> {
        let runtime = self.runtime(job_id).await;
        let ownership = runtime.ownership.lock().await;
        self.reap_runtime_handle(&runtime).await;
        let target = if self.discovery_complete(job_id)? {
            JobState::Parsing
        } else {
            JobState::Discovering
        };
        self.cas_state(job_id, JobState::Paused, target)?;
        runtime.stop.store(false, Ordering::Release);
        self.install_worker(job_id.to_owned(), &runtime).await?;
        drop(ownership);
        self.emit_status(job_id);
        Ok(())
    }

    pub async fn cancel(&self, job_id: &str) -> Result<(), IndexingError> {
        let runtime = self.runtime(job_id).await;
        self.cas_cancellable_state(job_id)?;
        runtime.stop.store(true, Ordering::Release);
        let ownership = runtime.ownership.lock().await;
        let gate = runtime.gate.lock().await;
        drop(gate);
        self.await_worker(&runtime).await;
        Self::remove_runtime_if_same(&self.runtimes, job_id, &runtime).await;
        drop(ownership);
        self.emit_status(job_id);
        Ok(())
    }

    pub async fn status(&self, job_id: &str) -> Result<IndexStatus, IndexingError> {
        self.reap_finished(job_id).await;
        self.load_status(job_id)
    }

    pub async fn shutdown_local(&self, job_id: &str) {
        if let Some(runtime) = self.runtimes.lock().await.remove(job_id) {
            runtime.stop.store(true, Ordering::Release);
            let mut handle = runtime.handle.lock().await;
            if let Some(handle) = handle.take() {
                handle.abort();
                let _ = handle.await;
            }
        }
    }

    pub async fn shutdown_all(&self) {
        let job_ids = self
            .runtimes
            .lock()
            .await
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for job_id in job_ids {
            self.shutdown_local(&job_id).await;
        }
    }

    /// Stop every active indexing job for a folder before its database rows are
    /// removed.  A watcher can be dropped while a worker is still parsing; in
    /// that case the worker may otherwise write a document back while the
    /// folder-removal transaction is running.
    pub async fn cancel_for_folder(&self, folder_id: &str) -> Result<(), IndexingError> {
        let job_ids = {
            let connection = self.database.connection();
            let mut statement = connection.prepare(
                "SELECT id FROM index_jobs
                 WHERE folder_id = ?1
                   AND state IN ('queued', 'discovering', 'parsing', 'paused')
                 ORDER BY updated_at, id",
            )?;
            let rows = statement
                .query_map([folder_id], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };

        for job_id in job_ids {
            // The worker can finish between the query above and cancel(). A
            // terminal transition is harmless because the folder deletion is
            // still the final source of truth, so only propagate real errors.
            match self.cancel(&job_id).await {
                Ok(()) => {}
                Err(IndexingError::Transition { .. }) | Err(IndexingError::JobNotFound(_)) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    pub async fn recover(&self) -> Result<Vec<JobId>, IndexingError> {
        let jobs = {
            let connection = self.database.connection();
            let mut statement = connection.prepare(
                "SELECT id FROM index_jobs
                 WHERE state IN ('queued', 'discovering', 'parsing')
                 ORDER BY updated_at, id",
            )?;
            let jobs = statement
                .query_map([], |row| row.get(0))?
                .collect::<Result<Vec<String>, _>>()?;
            jobs
        };
        for job_id in &jobs {
            let changed = self.database.connection().execute(
                "UPDATE index_jobs
                 SET state = 'paused',
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1 AND state IN ('queued', 'discovering', 'parsing')",
                [job_id],
            )?;
            if changed == 1 {
                self.resume(job_id).await?;
            }
        }
        Ok(jobs)
    }

    async fn runtime(&self, job_id: &str) -> Arc<JobRuntime> {
        let mut runtimes = self.runtimes.lock().await;
        Arc::clone(
            runtimes
                .entry(job_id.to_owned())
                .or_insert_with(|| Arc::new(JobRuntime::new())),
        )
    }

    async fn remove_runtime_if_same(
        runtimes: &tokio::sync::Mutex<HashMap<JobId, Arc<JobRuntime>>>,
        job_id: &str,
        expected: &Arc<JobRuntime>,
    ) {
        let mut runtimes = runtimes.lock().await;
        if runtimes
            .get(job_id)
            .is_some_and(|current| Arc::ptr_eq(current, expected))
        {
            runtimes.remove(job_id);
        }
    }

    async fn await_worker(&self, runtime: &Arc<JobRuntime>) {
        let handle = runtime.handle.lock().await.take();
        if let Some(handle) = handle {
            let _ = handle.await;
        }
    }

    pub(crate) fn registered_root(&self, folder_id: &str) -> Result<PathBuf, IndexingError> {
        Ok(PathBuf::from(
            self.registered_folder(folder_id)?.canonical_path,
        ))
    }

    pub(crate) async fn reindex_discovered_path(
        &self,
        folder_id: &str,
        event_path: &Path,
    ) -> Result<(), IndexingError> {
        let folder = self.registered_folder(folder_id)?;
        let root = PathBuf::from(&folder.canonical_path);
        let event_identity = match trusted_event_identity(&root, event_path, &OsEventPathProvider)?
        {
            Some(path) => path,
            None => return Ok(()),
        };
        let discovery_options = self.discovery_options()?;
        let candidate = tokio::task::spawn_blocking(move || {
            let stream = discover(&folder, discovery_options)?;
            Ok::<_, IndexingError>(
                stream
                    .into_iter()
                    .find(|candidate| candidate.canonical_path == event_identity),
            )
        })
        .await
        .map_err(|_| IndexingError::WorkerStopped)??;
        if let Some(candidate) = candidate {
            if self.candidate_matches_stored_identity(folder_id, &candidate)? {
                return Ok(());
            }
            let job_id = random_id();
            self.create_single_candidate_job(&job_id, folder_id, candidate, "watcher")?;
            let runtime = Arc::new(JobRuntime::new());
            self.run_job(job_id, runtime).await;
        }
        Ok(())
    }

    fn candidate_matches_stored_identity(
        &self,
        folder_id: &str,
        candidate: &FileCandidate,
    ) -> Result<bool, IndexingError> {
        let stored: Option<(i64, String)> = self
            .database
            .connection()
            .query_row(
                "SELECT documents.size_bytes, documents.modified_at
                 FROM documents
                 WHERE documents.folder_id = ?1
                   AND documents.canonical_path = ?2
                   AND (
                      (
                        ?3
                        AND documents.parse_state = 'metadata_only'
                        AND documents.parse_attempt_token IS NULL
                      )
                      OR (
                        NOT ?3
                        AND documents.parse_state = 'parsed'
                        AND documents.parse_attempt_token IS NULL
                        AND EXISTS (
                         SELECT 1 FROM document_content
                         WHERE document_content.document_id = documents.id
                       )
                       AND EXISTS (
                         SELECT 1 FROM document_fts
                         WHERE document_fts.document_id = documents.id
                       )
                     )
                   )",
                params![
                    folder_id,
                    candidate.canonical_path.to_string_lossy(),
                    candidate.metadata_only
                ],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        Ok(stored
            == Some((
                i64::try_from(candidate.size_bytes).unwrap_or(i64::MAX),
                modified_at_string(candidate.modified_at),
            )))
    }

    pub(crate) async fn reconcile_rename(
        &self,
        folder_id: &str,
        from: &Path,
        to: &Path,
    ) -> Result<(), IndexingError> {
        let trusted_to = match self.trusted_existing_path(folder_id, to)? {
            Some(path) => path,
            None => return Ok(()),
        };
        let metadata =
            fs::metadata(&trusted_to).map_err(|source| IndexingError::PathUnavailable {
                path: trusted_to.to_string_lossy().into_owned(),
                source,
            })?;
        let new_size = i64::try_from(metadata.len()).unwrap_or(i64::MAX);
        let new_modified = modified_at_string(metadata.modified().ok());
        let root = self.registered_root(folder_id)?;
        let Some(trusted_from) = trusted_event_identity(&root, from, &OsEventPathProvider)? else {
            self.reindex_discovered_path(folder_id, &trusted_to).await?;
            return Ok(());
        };
        let Some(old) = self.stored_path_for_event(folder_id, &trusted_from)? else {
            self.reindex_discovered_path(folder_id, &trusted_to).await?;
            return Ok(());
        };
        let new = trusted_to.to_string_lossy().into_owned();
        let new_name = trusted_to
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        let new_extension = trusted_to
            .extension()
            .map(|value| value.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();

        let (matched, invalidated_active_attempt) = {
            let mut connection = self.database.connection();
            let transaction = connection.transaction()?;
            let identity: Option<(String, i64, String, bool)> = transaction
                .query_row(
                    "SELECT id, size_bytes, modified_at,
                            parse_state = 'parsing' OR parse_attempt_token IS NOT NULL
                     FROM documents
                     WHERE folder_id = ?1 AND canonical_path = ?2",
                    params![folder_id, old],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()?;
            let matched = identity.as_ref().is_some_and(|(_, size, modified, _)| {
                (*size, modified.as_str()) == (new_size, new_modified.as_str())
            });
            let invalidated_active_attempt = matched
                && identity
                    .as_ref()
                    .is_some_and(|(_, _, _, active_attempt)| *active_attempt);
            if matched {
                let document_id = &identity.as_ref().expect("matched identity").0;
                transaction.execute(
                    "DELETE FROM document_fts
                     WHERE document_id IN (
                       SELECT id FROM documents
                       WHERE folder_id = ?1 AND canonical_path = ?2 AND id <> ?3
                     )",
                    params![folder_id, new, document_id],
                )?;
                transaction.execute(
                    "DELETE FROM documents
                     WHERE folder_id = ?1 AND canonical_path = ?2 AND id <> ?3",
                    params![folder_id, new, document_id],
                )?;
                transaction.execute(
                    "UPDATE documents
                     SET canonical_path = ?3,
                         file_name = ?4,
                         extension = ?5,
                         parse_state = CASE
                           WHEN parse_state = 'parsing'
                             OR parse_attempt_token IS NOT NULL
                           THEN 'pending'
                           ELSE parse_state
                         END,
                         parse_error_code = CASE
                           WHEN parse_state = 'parsing'
                             OR parse_attempt_token IS NOT NULL
                           THEN NULL
                           ELSE parse_error_code
                         END,
                         parse_attempt_token = NULL
                     WHERE folder_id = ?1 AND canonical_path = ?2",
                    params![folder_id, old, new, new_name, new_extension],
                )?;
                transaction.execute(
                    "UPDATE document_fts SET file_name = ?2 WHERE document_id = ?1",
                    params![document_id, new_name],
                )?;
            }
            transaction.commit()?;
            (matched, invalidated_active_attempt)
        };
        if !matched {
            self.delete_stored_document(folder_id, &old)?;
            self.reindex_discovered_path(folder_id, &trusted_to).await?;
        } else if invalidated_active_attempt {
            self.reindex_discovered_path(folder_id, &trusted_to).await?;
        }
        Ok(())
    }

    pub(crate) fn delete_document(
        &self,
        folder_id: &str,
        path: &Path,
    ) -> Result<(), IndexingError> {
        let root = self.registered_root(folder_id)?;
        if path.exists() {
            if self.trusted_existing_path(folder_id, path)?.is_none() {
                return Ok(());
            }
        } else if !lexically_within(&root, path) {
            return Ok(());
        }
        let Some(canonical_path) = self.stored_path_for_event(folder_id, path)? else {
            return Ok(());
        };
        self.delete_stored_document(folder_id, &canonical_path)
    }

    fn delete_stored_document(
        &self,
        folder_id: &str,
        canonical_path: &str,
    ) -> Result<(), IndexingError> {
        let mut connection = self.database.connection();
        let transaction = connection.transaction()?;
        transaction.execute(
            "DELETE FROM document_fts
             WHERE document_id IN (
               SELECT id FROM documents WHERE folder_id = ?1 AND canonical_path = ?2
             )",
            params![folder_id, canonical_path],
        )?;
        transaction.execute(
            "DELETE FROM documents WHERE folder_id = ?1 AND canonical_path = ?2",
            params![folder_id, canonical_path],
        )?;
        transaction.commit()?;
        Ok(())
    }

    fn stored_path_for_event(
        &self,
        folder_id: &str,
        event_path: &Path,
    ) -> Result<Option<String>, IndexingError> {
        let event_key = super::watcher::normalized_path_key(event_path);
        let connection = self.database.connection();
        let mut statement = connection.prepare(
            "SELECT canonical_path FROM documents
             WHERE folder_id = ?1 ORDER BY canonical_path",
        )?;
        let mut rows = statement.query([folder_id])?;
        while let Some(row) = rows.next()? {
            let stored = row.get::<_, String>(0)?;
            if super::watcher::normalized_path_key(Path::new(&stored)) == event_key {
                return Ok(Some(stored));
            }
        }
        Ok(None)
    }

    pub(crate) fn record_folder_diagnostic(
        &self,
        folder_id: &str,
        code: &str,
        message: &str,
    ) -> Result<(), IndexingError> {
        let connection = self.database.connection();
        let job_id: Option<String> = connection
            .query_row(
                "SELECT id FROM index_jobs
                 WHERE folder_id = ?1 ORDER BY updated_at DESC, id DESC LIMIT 1",
                [folder_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(job_id) = job_id {
            connection.execute(
                "INSERT INTO index_job_errors
                 (id, job_id, file_name, code, message, created_at)
                 VALUES (?1, ?2, '', ?3, ?4,
                         strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                params![random_id(), job_id, code, message],
            )?;
        }
        Ok(())
    }

    pub async fn reconcile(&self, folder_id: &str) -> Result<(), IndexingError> {
        let coordinator = self.clone();
        let folder_id = folder_id.to_owned();
        let runtime = tokio::runtime::Handle::current();
        let control = Arc::new(ReconciliationControl::new(Arc::clone(
            &self.discovery_probe,
        )));
        let _owner = ReconciliationOwner::new(Arc::clone(&control));
        tokio::task::spawn_blocking(move || {
            let _finished = ReconciliationFinished::new(Arc::clone(&control));
            coordinator.reconcile_blocking(&folder_id, &runtime, &control)
        })
        .await
        .map_err(|_| IndexingError::WorkerStopped)?
    }

    fn reconcile_blocking(
        &self,
        folder_id: &str,
        runtime: &tokio::runtime::Handle,
        control: &Arc<ReconciliationControl>,
    ) -> Result<(), IndexingError> {
        control.checkpoint()?;
        let folder = self.registered_folder(folder_id)?;
        control.checkpoint()?;
        let scratch = control.with_mutation(|| {
            ReconciliationScratch::begin(Arc::clone(&self.database), folder_id)
        })?;
        control.checkpoint()?;
        let mut stream = discover(&folder, self.discovery_options()?)?;
        let discovery_result = (|| {
            loop {
                control.checkpoint()?;
                let candidate = match stream.next_with_timeout(Duration::from_millis(10)) {
                    DiscoveryPoll::Candidate(candidate) => candidate,
                    DiscoveryPoll::Pending => continue,
                    DiscoveryPoll::Finished => break,
                };
                control.checkpoint()?;
                self.discovery_probe
                    .before_reconciliation_candidate_mutation();
                control.with_mutation(|| scratch.record(&candidate.canonical_path))?;
                self.discovery_probe.candidate_persisted(1);

                let job_id = random_id();
                let job_runtime = Arc::new(JobRuntime::for_reconciliation(
                    control.reconciliation_gate(),
                ));
                let created = control.with_direct_job_mutation(&job_runtime, || {
                    if self.candidate_matches_stored_identity(folder_id, &candidate)? {
                        return Ok(false);
                    }
                    self.create_single_candidate_job(
                        &job_id,
                        folder_id,
                        candidate,
                        "reconciliation",
                    )?;
                    Ok(true)
                })?;
                if !created {
                    continue;
                }
                runtime.block_on(self.run_job(job_id.clone(), Arc::clone(&job_runtime)));
                control.clear_active_runtime(&job_runtime);
                if let Err(error) = control.checkpoint() {
                    let _ = self.cas_cancellable_state(&job_id);
                    return Err(error);
                }
            }
            Ok::<(), IndexingError>(())
        })();
        stream.cancel_and_join();
        discovery_result?;

        loop {
            control.checkpoint()?;
            let Some(path) = scratch.first_missing()? else {
                break;
            };
            control.checkpoint()?;
            self.discovery_probe.before_reconciliation_stale_delete();
            control.with_mutation(|| self.delete_document(folder_id, Path::new(&path)))?;
        }
        Ok(())
    }

    async fn spawn_job(&self, job_id: JobId) -> Result<(), IndexingError> {
        let runtime = self.runtime(&job_id).await;
        let _ownership = runtime.ownership.lock().await;
        self.reap_runtime_handle(&runtime).await;
        if matches!(self.job_state(&job_id), Ok(JobState::Queued)) {
            self.cas_state(&job_id, JobState::Queued, JobState::Discovering)?;
        }
        self.install_worker(job_id, &runtime).await
    }

    async fn install_worker(
        &self,
        job_id: JobId,
        runtime: &Arc<JobRuntime>,
    ) -> Result<(), IndexingError> {
        let mut handle = runtime.handle.lock().await;
        if handle.is_some() {
            return Err(IndexingError::AlreadyRunning(job_id));
        }
        runtime.stop.store(false, Ordering::Release);
        let coordinator = self.clone();
        let worker_job_id = job_id.clone();
        let worker_runtime = Arc::clone(runtime);
        let registered_runtime = Arc::clone(runtime);
        *handle = Some(tokio::spawn(async move {
            coordinator.run_job(worker_job_id, worker_runtime).await;
            if coordinator.job_state(&job_id).is_ok_and(|state| {
                matches!(
                    state,
                    JobState::Completed | JobState::Cancelled | JobState::Failed
                )
            }) {
                Self::remove_runtime_if_same(&coordinator.runtimes, &job_id, &registered_runtime)
                    .await;
            }
        }));
        Ok(())
    }

    async fn run_job(&self, job_id: JobId, runtime: Arc<JobRuntime>) {
        if matches!(self.job_state(&job_id), Ok(JobState::Queued))
            && self
                .cas_state(&job_id, JobState::Queued, JobState::Discovering)
                .is_err()
        {
            return;
        }
        self.emit_status(&job_id);

        let mut discovery_task = None;
        if matches!(self.job_state(&job_id), Ok(JobState::Discovering)) {
            let coordinator = self.clone();
            let discovery_job_id = job_id.clone();
            let discovery_runtime = Arc::clone(&runtime);
            discovery_task = Some(tokio::spawn(async move {
                coordinator
                    .run_discovery(&discovery_job_id, &discovery_runtime)
                    .await
            }));

            // Discovery promotes the job to `parsing` as soon as its first
            // metadata batch is committed. That lets the parser pipeline
            // start while the directory walk is still producing candidates.
            loop {
                if runtime.stop.load(Ordering::Acquire)
                    || !matches!(self.job_state(&job_id), Ok(JobState::Discovering))
                {
                    break;
                }
                if discovery_task
                    .as_ref()
                    .is_some_and(|handle| handle.is_finished())
                {
                    let result = discovery_task
                        .take()
                        .expect("discovery task must exist")
                        .await
                        .unwrap_or(Err(IndexingError::WorkerStopped));
                    if let Err(error) = result {
                        if !runtime.stop.load(Ordering::Acquire) {
                            let _ = runtime.with_mutation(|| self.fail_active_job(&job_id, &error));
                        }
                        self.emit_status(&job_id);
                        return;
                    }
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
        if runtime.stop.load(Ordering::Acquire)
            || !matches!(self.job_state(&job_id), Ok(JobState::Parsing))
        {
            if let Some(task) = discovery_task {
                let _ = task.await;
            }
            return;
        }

        reset_in_flight_candidates(&self.database, &job_id);
        let (sender, mut receiver) = mpsc::channel::<PersistedCandidate>(PIPELINE_CAPACITY);
        let database = Arc::clone(&self.database);
        let producer_job_id = job_id.clone();
        let producer = tokio::task::spawn_blocking(move || {
            send_pending_candidates(&database, &producer_job_id, sender)
        });

        let mut workers = JoinSet::new();
        let mut input_open = true;
        let mut fatal_error = None;
        while input_open || !workers.is_empty() {
            while input_open
                && workers.len() < MAX_PARSER_CONCURRENCY
                && !runtime.stop.load(Ordering::Acquire)
            {
                match receiver.recv().await {
                    Some(candidate) => {
                        let coordinator = self.clone();
                        let worker_job_id = job_id.clone();
                        let worker_runtime = Arc::clone(&runtime);
                        workers.spawn(async move {
                            coordinator.limiter.yield_to_foreground().await;
                            coordinator
                                .process_candidate(&worker_job_id, candidate, &worker_runtime)
                                .await
                        });
                    }
                    None => input_open = false,
                }
            }

            if runtime.stop.load(Ordering::Acquire) {
                input_open = false;
            }
            let Some(result) = workers.join_next().await else {
                break;
            };
            let result = result.unwrap_or(Err(IndexingError::WorkerStopped));
            match result {
                Ok(()) => self.emit_status(&job_id),
                Err(IndexingError::StateChanged) if runtime.stop.load(Ordering::Acquire) => {}
                Err(error) => {
                    if fatal_error.is_none() {
                        fatal_error = Some(error);
                        runtime.stop.store(true, Ordering::Release);
                        input_open = false;
                    }
                }
            }
        }
        drop(receiver);
        let _ = producer.await;

        if let Some(task) = discovery_task {
            let result = task.await.unwrap_or(Err(IndexingError::WorkerStopped));
            if let Err(error) = result {
                if !runtime.stop.load(Ordering::Acquire) {
                    let _ = runtime.with_mutation(|| self.fail_active_job(&job_id, &error));
                }
                self.emit_status(&job_id);
                return;
            }
        }

        if let Some(error) = fatal_error {
            runtime.stop.store(false, Ordering::Release);
            let _ = runtime.with_mutation(|| self.fail_active_job(&job_id, &error));
            runtime.stop.store(true, Ordering::Release);
            self.emit_status(&job_id);
            return;
        }

        if !runtime.stop.load(Ordering::Acquire) {
            let _ = runtime
                .with_mutation(|| self.cas_state(&job_id, JobState::Parsing, JobState::Completed));
            self.emit_status(&job_id);
        }
    }

    async fn run_discovery(
        &self,
        job_id: &str,
        runtime: &Arc<JobRuntime>,
    ) -> Result<(), IndexingError> {
        let folder_id = {
            self.database.connection().query_row(
                "SELECT folder_id FROM index_jobs WHERE id = ?1",
                [job_id],
                |row| row.get::<_, String>(0),
            )?
        };
        let folder = self.registered_folder(&folder_id)?;
        let database = Arc::clone(&self.database);
        let stop = Arc::clone(runtime);
        let probe = Arc::clone(&self.discovery_probe);
        let discovery_job_id = job_id.to_owned();
        let discovery_options = self.discovery_options()?;
        let status_coordinator = self.clone();
        tokio::task::spawn_blocking(move || {
            let stream = discover(&folder, discovery_options)?;
            let mut pending = Vec::with_capacity(DISCOVERY_PERSIST_BATCH_SIZE);
            let mut last_status = Instant::now();
            for candidate in stream {
                if stop.stop.load(Ordering::Acquire) {
                    return Ok::<(), IndexingError>(());
                }
                pending.push(candidate);
                if pending.len() < DISCOVERY_PERSIST_BATCH_SIZE {
                    continue;
                }
                let persisted =
                    persist_discovered_candidates(&database, &discovery_job_id, &pending)?;
                for _ in 0..persisted {
                    probe.candidate_persisted(1);
                }
                pending.clear();
                if persisted > 0 || last_status.elapsed() >= Duration::from_millis(250) {
                    status_coordinator.emit_status(&discovery_job_id);
                    last_status = Instant::now();
                }
            }
            if !pending.is_empty() {
                let persisted =
                    persist_discovered_candidates(&database, &discovery_job_id, &pending)?;
                for _ in 0..persisted {
                    probe.candidate_persisted(1);
                }
                status_coordinator.emit_status(&discovery_job_id);
            }
            if stop.stop.load(Ordering::Acquire) {
                return Ok(());
            }
            let mut connection = database.connection();
            let transaction = connection.transaction()?;
            transaction.execute(
                "UPDATE index_job_recovery SET discovery_complete = 1 WHERE job_id = ?1",
                [&discovery_job_id],
            )?;
            transaction.execute(
                "UPDATE index_jobs
                 SET state = 'parsing',
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1 AND state = 'discovering'",
                [&discovery_job_id],
            )?;
            transaction.commit()?;
            drop(connection);
            status_coordinator.emit_status(&discovery_job_id);
            Ok(())
        })
        .await
        .map_err(|_| IndexingError::WorkerStopped)?
    }

    async fn process_candidate(
        &self,
        job_id: &str,
        candidate: PersistedCandidate,
        runtime: &Arc<JobRuntime>,
    ) -> Result<(), IndexingError> {
        self.apply_intensity_delay().await?;
        let gate = runtime.gate.lock().await;
        if runtime.stop.load(Ordering::Acquire)
            || !matches!(self.job_state(job_id), Ok(JobState::Parsing))
        {
            return Err(IndexingError::StateChanged);
        }
        runtime.with_mutation(|| {
            self.set_current_path(job_id, &candidate.canonical_path.to_string_lossy())
        })?;
        let runtime_settings = self
            .runtime_settings
            .read()
            .map_err(|_| IndexingError::RuntimeSettingsUnavailable)?
            .clone();
        let extraction_required = !candidate.metadata_only
            && candidate_requires_extraction(&candidate.canonical_path, &runtime_settings);
        if self.candidate_is_current(&candidate, extraction_required)? {
            return runtime.with_mutation(|| self.complete_cached_candidate(job_id, &candidate));
        }
        if candidate.metadata_only
            || path_is_metadata_only(&candidate.canonical_path)?
            || !extraction_required
        {
            return runtime.with_mutation(|| self.complete_metadata_only(job_id, &candidate));
        }
        let trusted_path = match self.validate_immediately_before_parse(job_id, &candidate) {
            Ok(path) => path,
            Err(error) => {
                return runtime.with_mutation(|| {
                    self.complete_failure(
                        job_id,
                        &candidate,
                        "PATH_TRUST_FAILED",
                        &error.to_string(),
                    )
                });
            }
        };
        if runtime.stop.load(Ordering::Acquire) {
            return Err(IndexingError::StateChanged);
        }
        self.discovery_probe.before_reconciliation_parser_start();
        let (parser_operation, parsing_checkpoint) =
            runtime.begin_parser_operation(|| self.mark_document_parsing(&candidate))?;
        drop(gate);

        let parser = Arc::clone(&self.parser);
        let ocr = self.ocr.clone();
        let parse_path = trusted_path.clone();
        let tracks_ocr = should_track_ocr(&trusted_path, &runtime_settings);
        if tracks_ocr {
            self.begin_ocr_attempt(
                &parsing_checkpoint.document_id,
                &parsing_checkpoint.attempt_token,
                if runtime_settings.math_ocr_enabled
                    && trusted_path
                        .extension()
                        .and_then(|value| value.to_str())
                        .is_some_and(|value| value.eq_ignore_ascii_case("pdf"))
                {
                    "math"
                } else {
                    "text"
                },
            )?;
        }
        let cancelled = Arc::clone(&runtime.stop);
        let parsed = tokio::task::spawn_blocking(move || {
            extract_document(parser, ocr, &parse_path, &runtime_settings, cancelled)
        })
        .await;
        let attempt_result = runtime.finish_parser_operation(
            parser_operation,
            || match parsed {
                Ok(Ok(document)) => {
                    if tracks_ocr {
                        self.finish_ocr_attempt(
                            &parsing_checkpoint.document_id,
                            &parsing_checkpoint.attempt_token,
                            if document_was_ocr(&document) {
                                "completed"
                            } else {
                                "skipped"
                            },
                            None,
                        )?;
                    }
                    self.complete_success(
                        job_id,
                        &candidate,
                        &parsing_checkpoint.attempt_token,
                        document,
                    )
                }
                Ok(Err(error)) => {
                    let (code, message) = extraction_failure(&error);
                    if tracks_ocr {
                        self.finish_ocr_attempt(
                            &parsing_checkpoint.document_id,
                            &parsing_checkpoint.attempt_token,
                            if matches!(error, ExtractionError::Ocr(OcrError::Cancelled)) {
                                "cancelled"
                            } else {
                                "failed"
                            },
                            Some(code),
                        )?;
                    }
                    self.complete_parser_failure(
                        job_id,
                        &candidate,
                        &parsing_checkpoint.attempt_token,
                        code,
                        &message,
                    )
                }
                Err(_) => {
                    if tracks_ocr {
                        self.finish_ocr_attempt(
                            &parsing_checkpoint.document_id,
                            &parsing_checkpoint.attempt_token,
                            "failed",
                            Some("PARSER_WORKER_STOPPED"),
                        )?;
                    }
                    self.complete_parser_failure(
                        job_id,
                        &candidate,
                        &parsing_checkpoint.attempt_token,
                        "PARSER_WORKER_STOPPED",
                        "parser worker stopped unexpectedly",
                    )
                }
            },
            || {
                if tracks_ocr {
                    self.finish_ocr_attempt(
                        &parsing_checkpoint.document_id,
                        &parsing_checkpoint.attempt_token,
                        "cancelled",
                        Some("OCR_CANCELLED"),
                    )?;
                }
                self.compensate_cancelled_parse(&candidate, &parsing_checkpoint)
                    .map(|_| ())
            },
        )?;
        match attempt_result {
            ParseAttemptMutation::Applied => Ok(()),
            ParseAttemptMutation::Stale => self.complete_stale_attempt(job_id, &candidate),
        }
    }

    fn candidate_is_current(
        &self,
        candidate: &PersistedCandidate,
        extraction_required: bool,
    ) -> Result<bool, IndexingError> {
        let connection = self.database.connection();
        let Some((size_bytes, modified_at, parse_state, has_content, has_fts)) = connection
            .query_row(
                "SELECT d.size_bytes, d.modified_at, d.parse_state,
                        EXISTS (
                          SELECT 1 FROM document_content c WHERE c.document_id = d.id
                        ),
                        EXISTS (
                          SELECT 1 FROM document_fts f WHERE f.document_id = d.id
                        )
                 FROM documents d WHERE d.canonical_path = ?1",
                [candidate.canonical_path.to_string_lossy()],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, bool>(3)?,
                        row.get::<_, bool>(4)?,
                    ))
                },
            )
            .optional()?
        else {
            return Ok(false);
        };
        if size_bytes != candidate.size_bytes || modified_at != candidate.modified_at {
            return Ok(false);
        }
        if extraction_required {
            Ok(parse_state == "parsed" && has_content && has_fts)
        } else {
            Ok(parse_state == "metadata_only")
        }
    }

    fn complete_cached_candidate(
        &self,
        job_id: &str,
        candidate: &PersistedCandidate,
    ) -> Result<(), IndexingError> {
        let mut connection = self.database.connection();
        let transaction = connection.transaction()?;
        ensure_job_parsing(&transaction, job_id)?;
        advance_file_transaction(&transaction, job_id, candidate)?;
        transaction.commit()?;
        Ok(())
    }

    fn validate_immediately_before_parse(
        &self,
        job_id: &str,
        candidate: &PersistedCandidate,
    ) -> Result<PathBuf, IndexingError> {
        let folder_id: String = self.database.connection().query_row(
            "SELECT folder_id FROM index_jobs WHERE id = ?1",
            [job_id],
            |row| row.get(0),
        )?;
        let root = self
            .registered_root(&folder_id)?
            .canonicalize()
            .map_err(|source| IndexingError::PathUnavailable {
                path: folder_id,
                source,
            })?;
        if path_is_metadata_only(&candidate.canonical_path)? {
            return Err(IndexingError::MetadataOnly);
        }
        let canonical = candidate.canonical_path.canonicalize().map_err(|source| {
            IndexingError::PathUnavailable {
                path: candidate.relative_path.clone(),
                source,
            }
        })?;
        if !canonical.starts_with(&root) || canonical != candidate.canonical_path {
            return Err(IndexingError::PathOutsideRoot);
        }
        Ok(canonical)
    }

    fn create_single_candidate_job(
        &self,
        job_id: &str,
        folder_id: &str,
        candidate: FileCandidate,
        origin: &str,
    ) -> Result<(), IndexingError> {
        let mut connection = self.database.connection();
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO index_jobs
             (id, folder_id, state, completed_files, total_files, last_path, updated_at, origin)
             VALUES (?1, ?2, 'parsing', 0, 1, NULL,
                     strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), ?3)",
            params![job_id, folder_id, origin],
        )?;
        transaction.execute(
            "INSERT INTO index_job_files
             (job_id, canonical_path, relative_path, size_bytes, modified_at,
              metadata_only, state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'queued')",
            params![
                job_id,
                candidate.canonical_path.to_string_lossy(),
                candidate.relative_path,
                i64::try_from(candidate.size_bytes).unwrap_or(i64::MAX),
                modified_at_string(candidate.modified_at),
                candidate.metadata_only,
            ],
        )?;
        transaction.execute(
            "INSERT INTO index_job_recovery (job_id, discovery_complete)
             VALUES (?1, 1)",
            [job_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    fn registered_folder(&self, folder_id: &str) -> Result<FolderRecord, IndexingError> {
        self.database
            .connection()
            .query_row(
                "SELECT id, canonical_path, display_name
                 FROM folders WHERE id = ?1 AND enabled = 1",
                [folder_id],
                |row| {
                    Ok(FolderRecord {
                        id: row.get(0)?,
                        canonical_path: row.get(1)?,
                        display_name: row.get(2)?,
                        document_count: 0,
                        index_state: "idle".into(),
                    })
                },
            )
            .optional()?
            .ok_or_else(|| IndexingError::FolderNotFound(folder_id.to_owned()))
    }

    fn trusted_existing_path(
        &self,
        folder_id: &str,
        path: &Path,
    ) -> Result<Option<PathBuf>, IndexingError> {
        if path_is_metadata_only(path)? {
            return Ok(None);
        }
        let root = self
            .registered_root(folder_id)?
            .canonicalize()
            .map_err(|source| IndexingError::PathUnavailable {
                path: folder_id.to_owned(),
                source,
            })?;
        let canonical = match path.canonicalize() {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };
        Ok(canonical.starts_with(root).then_some(canonical))
    }

    fn mark_document_parsing(
        &self,
        candidate: &PersistedCandidate,
    ) -> Result<DocumentParsingCheckpoint, IndexingError> {
        let mut connection = self.database.connection();
        let transaction = connection.transaction()?;
        let folder_id = folder_id_for_job(&transaction, &candidate.job_id)?;
        let previous = transaction
            .query_row(
                "SELECT id, folder_id, file_name, extension, size_bytes, modified_at,
                        parse_state, parse_error_code
                 FROM documents WHERE canonical_path = ?1",
                [candidate.canonical_path.to_string_lossy()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        DocumentMetadataSnapshot {
                            folder_id: row.get(1)?,
                            file_name: row.get(2)?,
                            extension: row.get(3)?,
                            size_bytes: row.get(4)?,
                            modified_at: row.get(5)?,
                            parse_state: row.get(6)?,
                            parse_error_code: row.get(7)?,
                        },
                    ))
                },
            )
            .optional()?;
        let mut claimed = None;
        for claim in 0..MAX_PARSE_ATTEMPT_TOKEN_CLAIMS {
            let attempt_token = self.attempt_tokens.generate();
            match upsert_metadata_transaction(
                &transaction,
                &folder_id,
                candidate,
                "parsing",
                None,
                Some(&attempt_token),
                true,
            ) {
                Ok(mutation) => {
                    claimed = Some((attempt_token, mutation));
                    break;
                }
                Err(error)
                    if claim + 1 < MAX_PARSE_ATTEMPT_TOKEN_CLAIMS
                        && is_parse_attempt_token_collision(&error) =>
                {
                    continue;
                }
                Err(error) => return Err(error),
            }
        }
        let (attempt_token, mutation) = claimed.ok_or(IndexingError::StateChanged)?;
        if mutation != ParseAttemptMutation::Applied {
            return Err(IndexingError::StateChanged);
        }
        let document_id = match &previous {
            Some((document_id, _)) => document_id.clone(),
            None => transaction.query_row(
                "SELECT id FROM documents WHERE canonical_path = ?1",
                [candidate.canonical_path.to_string_lossy()],
                |row| row.get(0),
            )?,
        };
        transaction.commit()?;
        Ok(DocumentParsingCheckpoint {
            document_id,
            attempt_token,
            previous: previous.map(|(_, metadata)| metadata),
        })
    }

    fn compensate_cancelled_parse(
        &self,
        candidate: &PersistedCandidate,
        checkpoint: &DocumentParsingCheckpoint,
    ) -> Result<ParseAttemptMutation, IndexingError> {
        let mut connection = self.database.connection();
        let transaction = connection.transaction()?;
        let still_owned: bool = transaction.query_row(
            "SELECT EXISTS (
               SELECT 1 FROM documents
               WHERE id = ?1
                  AND canonical_path = ?2
                  AND parse_state = 'parsing'
                  AND parse_attempt_token = ?3
             )",
            params![
                checkpoint.document_id,
                candidate.canonical_path.to_string_lossy(),
                checkpoint.attempt_token,
            ],
            |row| row.get(0),
        )?;
        if !still_owned {
            transaction.commit()?;
            return Ok(ParseAttemptMutation::Stale);
        }

        if let Some(previous) = &checkpoint.previous {
            let restored_state = if previous.parse_state == "parsing" {
                "pending"
            } else {
                &previous.parse_state
            };
            let restored_error = if previous.parse_state == "parsing" {
                None
            } else {
                previous.parse_error_code.as_deref()
            };
            let changed = transaction.execute(
                "UPDATE documents
                 SET folder_id = ?2,
                     file_name = ?3,
                     extension = ?4,
                     size_bytes = ?5,
                     modified_at = ?6,
                     parse_state = ?7,
                     parse_error_code = ?8,
                     parse_attempt_token = NULL
                 WHERE id = ?1
                   AND canonical_path = ?9
                   AND parse_state = 'parsing'
                   AND parse_attempt_token = ?10",
                params![
                    checkpoint.document_id,
                    previous.folder_id,
                    previous.file_name,
                    previous.extension,
                    previous.size_bytes,
                    previous.modified_at,
                    restored_state,
                    restored_error,
                    candidate.canonical_path.to_string_lossy(),
                    checkpoint.attempt_token,
                ],
            )?;
            if changed != 1 {
                return Ok(ParseAttemptMutation::Stale);
            }
        } else {
            transaction.execute(
                "DELETE FROM document_fts
                 WHERE document_id = ?1
                   AND EXISTS (
                     SELECT 1 FROM documents
                     WHERE id = ?1
                       AND canonical_path = ?2
                       AND parse_state = 'parsing'
                       AND parse_attempt_token = ?3
                   )",
                params![
                    checkpoint.document_id,
                    candidate.canonical_path.to_string_lossy(),
                    checkpoint.attempt_token,
                ],
            )?;
            let changed = transaction.execute(
                "DELETE FROM documents
                 WHERE id = ?1
                   AND canonical_path = ?2
                   AND parse_state = 'parsing'
                   AND parse_attempt_token = ?3",
                params![
                    checkpoint.document_id,
                    candidate.canonical_path.to_string_lossy(),
                    checkpoint.attempt_token,
                ],
            )?;
            if changed != 1 {
                return Ok(ParseAttemptMutation::Stale);
            }
        }
        transaction.commit()?;
        Ok(ParseAttemptMutation::Applied)
    }

    fn begin_ocr_attempt(
        &self,
        document_id: &str,
        attempt_token: &str,
        model_kind: &str,
    ) -> Result<(), IndexingError> {
        self.database.connection().execute(
            "INSERT INTO ocr_attempts (
               document_id, attempt_token, state, engine, model_kind, started_at, finished_at
             ) VALUES (
               ?1, ?2, 'running', 'paddleocr', ?3,
               strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), NULL
             )
             ON CONFLICT(document_id) DO UPDATE SET
               attempt_token = excluded.attempt_token,
               state = 'running',
               engine = excluded.engine,
               model_kind = excluded.model_kind,
               error_code = NULL,
               started_at = excluded.started_at,
               finished_at = NULL
             WHERE ocr_attempts.attempt_token IS NULL
                OR ocr_attempts.state IN ('completed', 'skipped', 'failed', 'cancelled')",
            params![document_id, attempt_token, model_kind],
        )?;
        Ok(())
    }

    fn finish_ocr_attempt(
        &self,
        document_id: &str,
        attempt_token: &str,
        state: &str,
        error_code: Option<&str>,
    ) -> Result<(), IndexingError> {
        let changed = self.database.connection().execute(
            "UPDATE ocr_attempts
             SET state = ?3,
                 error_code = ?4,
                 finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                 attempt_token = NULL
             WHERE document_id = ?1 AND attempt_token = ?2",
            params![document_id, attempt_token, state, error_code],
        )?;
        if changed == 1 {
            Ok(())
        } else {
            Err(IndexingError::StateChanged)
        }
    }

    fn complete_metadata_only(
        &self,
        job_id: &str,
        candidate: &PersistedCandidate,
    ) -> Result<(), IndexingError> {
        let mut connection = self.database.connection();
        let transaction = connection.transaction()?;
        ensure_job_parsing(&transaction, job_id)?;
        let folder_id = folder_id_for_job(&transaction, job_id)?;
        upsert_metadata_transaction(
            &transaction,
            &folder_id,
            candidate,
            "metadata_only",
            None,
            None,
            false,
        )?;
        advance_file_transaction(&transaction, job_id, candidate)?;
        transaction.commit()?;
        Ok(())
    }

    fn complete_success(
        &self,
        job_id: &str,
        candidate: &PersistedCandidate,
        attempt_token: &str,
        document: ParsedDocument,
    ) -> Result<ParseAttemptMutation, IndexingError> {
        let mut connection = self.database.connection();
        let transaction = connection.transaction()?;
        ensure_job_parsing(&transaction, job_id)?;
        let document_id = transaction
            .query_row(
                "SELECT id FROM documents
                 WHERE canonical_path = ?1
                   AND parse_state = 'parsing'
                   AND parse_attempt_token = ?2",
                params![candidate.canonical_path.to_string_lossy(), attempt_token],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(document_id) = document_id else {
            transaction.commit()?;
            return Ok(ParseAttemptMutation::Stale);
        };
        transaction.execute(
            "DELETE FROM document_content WHERE document_id = ?1",
            [&document_id],
        )?;
        transaction.execute(
            "INSERT INTO document_content
             (document_id, title, body, markdown, blocks_json, warnings_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                document_id,
                document.title,
                document.plain_text,
                document.markdown,
                serde_json::to_string(&document.blocks)?,
                serde_json::to_string(&document.warnings)?,
            ],
        )?;
        transaction.execute(
            "DELETE FROM document_fts WHERE document_id = ?1",
            [&document_id],
        )?;
        transaction.execute(
            "INSERT INTO document_fts (document_id, file_name, title, body)
             SELECT id, file_name, ?2, ?3 FROM documents WHERE id = ?1",
            params![document_id, document.title, document.plain_text],
        )?;
        let changed = transaction.execute(
            "UPDATE documents
             SET parse_state = 'parsed', parse_error_code = NULL,
                 indexed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                 parse_attempt_token = NULL
             WHERE id = ?1
               AND parse_state = 'parsing'
               AND parse_attempt_token = ?2",
            params![document_id, attempt_token],
        )?;
        if changed != 1 {
            return Ok(ParseAttemptMutation::Stale);
        }
        advance_file_transaction(&transaction, job_id, candidate)?;
        transaction.commit()?;
        Ok(ParseAttemptMutation::Applied)
    }

    fn complete_parser_failure(
        &self,
        job_id: &str,
        candidate: &PersistedCandidate,
        attempt_token: &str,
        code: &str,
        message: &str,
    ) -> Result<ParseAttemptMutation, IndexingError> {
        let mut connection = self.database.connection();
        let transaction = connection.transaction()?;
        ensure_job_parsing(&transaction, job_id)?;
        let changed = transaction.execute(
            "UPDATE documents
             SET parse_state = 'failed',
                 parse_error_code = ?3,
                 parse_attempt_token = NULL
             WHERE canonical_path = ?1
               AND parse_state = 'parsing'
               AND parse_attempt_token = ?2",
            params![
                candidate.canonical_path.to_string_lossy(),
                attempt_token,
                code
            ],
        )?;
        if changed != 1 {
            transaction.commit()?;
            return Ok(ParseAttemptMutation::Stale);
        }
        transaction.execute(
            "INSERT INTO index_job_errors
             (id, job_id, file_name, code, message, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5,
                     strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            params![
                random_id(),
                job_id,
                file_name(&candidate.canonical_path),
                code,
                message
            ],
        )?;
        advance_file_transaction(&transaction, job_id, candidate)?;
        transaction.commit()?;
        Ok(ParseAttemptMutation::Applied)
    }

    fn complete_failure(
        &self,
        job_id: &str,
        candidate: &PersistedCandidate,
        code: &str,
        message: &str,
    ) -> Result<(), IndexingError> {
        let mut connection = self.database.connection();
        let transaction = connection.transaction()?;
        ensure_job_parsing(&transaction, job_id)?;
        let folder_id = folder_id_for_job(&transaction, job_id)?;
        upsert_metadata_transaction(
            &transaction,
            &folder_id,
            candidate,
            "failed",
            Some(code),
            None,
            false,
        )?;
        transaction.execute(
            "INSERT INTO index_job_errors
             (id, job_id, file_name, code, message, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5,
                     strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            params![
                random_id(),
                job_id,
                file_name(&candidate.canonical_path),
                code,
                message
            ],
        )?;
        advance_file_transaction(&transaction, job_id, candidate)?;
        transaction.commit()?;
        Ok(())
    }

    fn complete_stale_attempt(
        &self,
        job_id: &str,
        candidate: &PersistedCandidate,
    ) -> Result<(), IndexingError> {
        let mut connection = self.database.connection();
        let transaction = connection.transaction()?;
        ensure_job_parsing(&transaction, job_id)?;
        advance_file_transaction(&transaction, job_id, candidate)?;
        transaction.commit()?;
        Ok(())
    }

    fn set_current_path(&self, job_id: &str, relative_path: &str) -> Result<(), IndexingError> {
        self.database.connection().execute(
            "UPDATE index_jobs
             SET last_path = ?2, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1",
            params![job_id, relative_path],
        )?;
        Ok(())
    }

    fn job_state(&self, job_id: &str) -> Result<JobState, IndexingError> {
        let value: String = self
            .database
            .connection()
            .query_row(
                "SELECT state FROM index_jobs WHERE id = ?1",
                [job_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| IndexingError::JobNotFound(job_id.to_owned()))?;
        JobState::from_sql(&value)
    }

    fn cas_state(
        &self,
        job_id: &str,
        expected: JobState,
        target: JobState,
    ) -> Result<(), IndexingError> {
        let changed = self.database.connection().execute(
            "UPDATE index_jobs
             SET state = ?3, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1 AND state = ?2",
            params![job_id, expected.as_sql(), target.as_sql()],
        )?;
        if changed == 1 {
            Ok(())
        } else {
            let from = self.job_state(job_id)?;
            Err(IndexingError::Transition { from, to: target })
        }
    }

    fn cas_active_state(&self, job_id: &str, target: JobState) -> Result<(), IndexingError> {
        let changed = self.database.connection().execute(
            "UPDATE index_jobs
             SET state = ?2, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1 AND state IN ('queued', 'discovering', 'parsing')",
            params![job_id, target.as_sql()],
        )?;
        if changed == 1 {
            Ok(())
        } else {
            let from = self.job_state(job_id)?;
            Err(IndexingError::Transition { from, to: target })
        }
    }

    fn cas_cancellable_state(&self, job_id: &str) -> Result<(), IndexingError> {
        let changed = self.database.connection().execute(
            "UPDATE index_jobs
             SET state = 'cancelled',
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1
               AND state IN ('queued', 'discovering', 'parsing', 'paused')",
            [job_id],
        )?;
        if changed == 1 {
            Ok(())
        } else {
            let from = self.job_state(job_id)?;
            Err(IndexingError::Transition {
                from,
                to: JobState::Cancelled,
            })
        }
    }

    fn fail_active_job(&self, job_id: &str, error: &IndexingError) -> Result<(), IndexingError> {
        let changed = self.database.connection().execute(
            "UPDATE index_jobs
             SET state = 'failed',
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1 AND state IN ('queued', 'discovering', 'parsing')",
            [job_id],
        )?;
        if changed == 1 {
            Ok(())
        } else {
            Err(IndexingError::WorkerFailure(error.to_string()))
        }
    }

    fn discovery_complete(&self, job_id: &str) -> Result<bool, IndexingError> {
        Ok(self.database.connection().query_row(
            "SELECT discovery_complete FROM index_job_recovery WHERE job_id = ?1",
            [job_id],
            |row| row.get::<_, bool>(0),
        )?)
    }

    async fn reap_runtime_handle(&self, runtime: &Arc<JobRuntime>) {
        let mut handle = runtime.handle.lock().await;
        if handle.as_ref().is_some_and(JoinHandle::is_finished) {
            if let Some(finished) = handle.take() {
                let _ = finished.await;
            }
        }
    }

    async fn reap_finished(&self, job_id: &str) {
        let runtime = self.runtimes.lock().await.get(job_id).cloned();
        if let Some(runtime) = runtime {
            let _ownership = runtime.ownership.lock().await;
            self.reap_runtime_handle(&runtime).await;
            let terminal = self.job_state(job_id).is_ok_and(|state| {
                matches!(
                    state,
                    JobState::Completed | JobState::Cancelled | JobState::Failed
                )
            });
            if terminal && runtime.handle.lock().await.is_none() {
                Self::remove_runtime_if_same(&self.runtimes, job_id, &runtime).await;
            }
        }
    }

    fn load_status(&self, job_id: &str) -> Result<IndexStatus, IndexingError> {
        let connection = self.database.connection();
        let row: Option<(String, i64, i64, Option<String>, String)> = connection
            .query_row(
                "SELECT state, total_files, completed_files, last_path, origin
                 FROM index_jobs WHERE id = ?1",
                [job_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?;
        let (state, total, completed, current_path, origin) =
            row.ok_or_else(|| IndexingError::JobNotFound(job_id.to_owned()))?;
        let state = JobState::from_sql(&state)?;
        let error_count = connection.query_row(
            "SELECT COUNT(*) FROM index_job_errors WHERE job_id = ?1",
            [job_id],
            |row| row.get::<_, i64>(0),
        )?;
        let errors = if matches!(
            state,
            JobState::Completed | JobState::Cancelled | JobState::Failed
        ) {
            let mut statement = connection.prepare(
                "SELECT code, file_name, message FROM index_job_errors
                 WHERE job_id = ?1 ORDER BY created_at, id LIMIT ?2",
            )?;
            let details = statement
                .query_map(params![job_id, MAX_TERMINAL_ERROR_DETAILS], |row| {
                    Ok(IndexFailure {
                        code: row.get(0)?,
                        file_name: row.get(1)?,
                        message: row.get(2)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            details
        } else {
            Vec::new()
        };
        Ok(IndexStatus {
            job_id: job_id.to_owned(),
            state,
            total_files: u64::try_from(total).unwrap_or(0),
            completed_files: u64::try_from(completed).unwrap_or(0),
            current_path,
            error_count: u64::try_from(error_count).unwrap_or(0),
            errors,
            silent: origin != "manual",
        })
    }

    fn emit_status(&self, job_id: &str) {
        if let (Some(sink), Ok(status)) = (&self.status_sink, self.load_status(job_id)) {
            if let Err(message) = sink(status) {
                let _ = self.database.connection().execute(
                    "INSERT INTO index_job_errors
                     (id, job_id, file_name, code, message, created_at)
                     VALUES (?1, ?2, '', 'STATUS_EVENT_EMIT_FAILED', ?3,
                             strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                    params![random_id(), job_id, message],
                );
            }
        }
    }
}

#[derive(Debug)]
struct PersistedCandidate {
    job_id: String,
    canonical_path: PathBuf,
    relative_path: String,
    size_bytes: i64,
    modified_at: String,
    metadata_only: bool,
}

struct DocumentParsingCheckpoint {
    document_id: String,
    attempt_token: String,
    previous: Option<DocumentMetadataSnapshot>,
}

struct DocumentMetadataSnapshot {
    folder_id: String,
    file_name: String,
    extension: String,
    size_bytes: i64,
    modified_at: String,
    parse_state: String,
    parse_error_code: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParseAttemptMutation {
    Applied,
    Stale,
}

struct ReconciliationMutationState {
    cancelled: bool,
    active_operations: usize,
    active_runtime: Option<Weak<JobRuntime>>,
}

struct ReconciliationMutationGate {
    state: Mutex<ReconciliationMutationState>,
    idle: Condvar,
}

impl ReconciliationMutationGate {
    fn new() -> Self {
        Self {
            state: Mutex::new(ReconciliationMutationState {
                cancelled: false,
                active_operations: 0,
                active_runtime: None,
            }),
            idle: Condvar::new(),
        }
    }

    fn checkpoint(&self) -> Result<(), IndexingError> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.cancelled {
            Err(IndexingError::ReconciliationCancelled)
        } else {
            Ok(())
        }
    }

    fn with_mutation<T>(
        &self,
        mutation: impl FnOnce() -> Result<T, IndexingError>,
    ) -> Result<T, IndexingError> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.cancelled {
            return Err(IndexingError::ReconciliationCancelled);
        }
        let result = mutation();
        drop(state);
        result
    }

    fn with_direct_job_mutation(
        &self,
        runtime: &Arc<JobRuntime>,
        mutation: impl FnOnce() -> Result<bool, IndexingError>,
    ) -> Result<bool, IndexingError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.cancelled {
            return Err(IndexingError::ReconciliationCancelled);
        }
        let created = mutation()?;
        if created {
            state.active_runtime = Some(Arc::downgrade(runtime));
        }
        Ok(created)
    }

    fn begin_operation<T>(
        self: &Arc<Self>,
        mutation: impl FnOnce() -> Result<T, IndexingError>,
    ) -> Result<(ActiveReconciliationOperation, T), IndexingError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.cancelled {
            return Err(IndexingError::ReconciliationCancelled);
        }
        let value = mutation()?;
        state.active_operations += 1;
        Ok((
            ActiveReconciliationOperation {
                gate: Arc::clone(self),
            },
            value,
        ))
    }

    fn clear_active_runtime(&self, runtime: &Arc<JobRuntime>) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state
            .active_runtime
            .as_ref()
            .and_then(Weak::upgrade)
            .is_some_and(|current| Arc::ptr_eq(&current, runtime))
        {
            state.active_runtime = None;
        }
    }

    fn publish_stop(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.cancelled = true;
        if let Some(runtime) = state.active_runtime.as_ref().and_then(Weak::upgrade) {
            runtime.stop.store(true, Ordering::Release);
        }
    }

    fn wait_until_idle(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while state.active_operations != 0 {
            state = self
                .idle
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    fn finish_operation(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        debug_assert!(state.active_operations > 0);
        state.active_operations = state.active_operations.saturating_sub(1);
        if state.active_operations == 0 {
            self.idle.notify_all();
        }
    }
}

struct ActiveReconciliationOperation {
    gate: Arc<ReconciliationMutationGate>,
}

impl ActiveReconciliationOperation {
    fn finish<T>(
        self,
        commit: impl FnOnce() -> Result<T, IndexingError>,
        compensate: impl FnOnce() -> Result<(), IndexingError>,
    ) -> Result<T, IndexingError> {
        let state = self
            .gate
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let result = if state.cancelled {
            compensate()?;
            Err(IndexingError::ReconciliationCancelled)
        } else {
            commit()
        };
        drop(state);
        result
    }
}

impl Drop for ActiveReconciliationOperation {
    fn drop(&mut self) {
        self.gate.finish_operation();
    }
}

struct ReconciliationControl {
    mutation_gate: Arc<ReconciliationMutationGate>,
    probe: Arc<dyn DiscoveryProbe>,
    finished: (Mutex<bool>, Condvar),
}

impl ReconciliationControl {
    fn new(probe: Arc<dyn DiscoveryProbe>) -> Self {
        Self {
            mutation_gate: Arc::new(ReconciliationMutationGate::new()),
            probe,
            finished: (Mutex::new(false), Condvar::new()),
        }
    }

    fn checkpoint(&self) -> Result<(), IndexingError> {
        self.mutation_gate.checkpoint()
    }

    fn with_mutation<T>(
        &self,
        mutation: impl FnOnce() -> Result<T, IndexingError>,
    ) -> Result<T, IndexingError> {
        self.mutation_gate.with_mutation(mutation)
    }

    fn with_direct_job_mutation(
        &self,
        runtime: &Arc<JobRuntime>,
        mutation: impl FnOnce() -> Result<bool, IndexingError>,
    ) -> Result<bool, IndexingError> {
        self.mutation_gate
            .with_direct_job_mutation(runtime, mutation)
    }

    fn reconciliation_gate(&self) -> Arc<ReconciliationMutationGate> {
        Arc::clone(&self.mutation_gate)
    }

    fn cancel(&self) {
        self.mutation_gate.publish_stop();
        self.probe.reconciliation_cancelled();
        self.mutation_gate.wait_until_idle();
    }

    fn clear_active_runtime(&self, runtime: &Arc<JobRuntime>) {
        self.mutation_gate.clear_active_runtime(runtime);
    }

    fn mark_finished(&self) {
        let (finished, ready) = &self.finished;
        *finished
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
        ready.notify_all();
    }

    fn wait_until_finished(&self) {
        let (finished, ready) = &self.finished;
        let mut finished = finished
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while !*finished {
            finished = ready
                .wait(finished)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }
}

struct ReconciliationOwner {
    control: Arc<ReconciliationControl>,
}

impl ReconciliationOwner {
    fn new(control: Arc<ReconciliationControl>) -> Self {
        Self { control }
    }
}

impl Drop for ReconciliationOwner {
    fn drop(&mut self) {
        self.control.cancel();
        self.control.wait_until_finished();
    }
}

struct ReconciliationFinished {
    control: Arc<ReconciliationControl>,
}

impl ReconciliationFinished {
    fn new(control: Arc<ReconciliationControl>) -> Self {
        Self { control }
    }
}

impl Drop for ReconciliationFinished {
    fn drop(&mut self) {
        self.control.mark_finished();
    }
}

struct ReconciliationScratch {
    database: Arc<Database>,
    run_id: String,
    folder_id: String,
}

impl ReconciliationScratch {
    fn begin(database: Arc<Database>, folder_id: &str) -> Result<Self, IndexingError> {
        let run_id = random_id();
        database.connection().execute(
            "INSERT INTO reconciliation_runs (id, folder_id, created_at)
             VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            params![run_id, folder_id],
        )?;
        Ok(Self {
            database,
            run_id,
            folder_id: folder_id.to_owned(),
        })
    }

    fn record(&self, path: &Path) -> Result<(), IndexingError> {
        self.database.connection().execute(
            "INSERT OR IGNORE INTO reconciliation_seen_v2 (run_id, canonical_path)
             VALUES (?1, ?2)",
            params![self.run_id, path.to_string_lossy()],
        )?;
        Ok(())
    }

    fn first_missing(&self) -> Result<Option<String>, IndexingError> {
        Ok(self
            .database
            .connection()
            .query_row(
                "SELECT canonical_path FROM documents
                 WHERE folder_id = ?1
                   AND NOT EXISTS (
                     SELECT 1 FROM reconciliation_seen_v2
                     WHERE reconciliation_seen_v2.run_id = ?2
                       AND reconciliation_seen_v2.canonical_path = documents.canonical_path
                   )
                 ORDER BY canonical_path LIMIT 1",
                params![self.folder_id, self.run_id],
                |row| row.get(0),
            )
            .optional()?)
    }
}

impl Drop for ReconciliationScratch {
    fn drop(&mut self) {
        let _ = self.database.connection().execute(
            "DELETE FROM reconciliation_runs WHERE id = ?1",
            [&self.run_id],
        );
    }
}

fn persist_discovered_candidates(
    database: &Database,
    job_id: &str,
    candidates: &[FileCandidate],
) -> Result<usize, IndexingError> {
    let mut connection = database.connection();
    let transaction = connection.transaction()?;
    let folder_id: String = transaction.query_row(
        "SELECT folder_id FROM index_jobs WHERE id = ?1",
        [job_id],
        |row| row.get(0),
    )?;
    let mut inserted_total = 0usize;
    for candidate in candidates {
        let inserted = transaction.execute(
            "INSERT OR IGNORE INTO index_job_files
             (job_id, canonical_path, relative_path, size_bytes, modified_at,
              metadata_only, state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'queued')",
            params![
                job_id,
                candidate.canonical_path.to_string_lossy(),
                candidate.relative_path,
                i64::try_from(candidate.size_bytes).unwrap_or(i64::MAX),
                modified_at_string(candidate.modified_at),
                candidate.metadata_only,
            ],
        )?;
        if inserted == 1 {
            inserted_total += 1;
        }
        upsert_discovered_metadata_transaction(&transaction, &folder_id, candidate)?;
    }
    if inserted_total > 0 {
        transaction.execute(
            "UPDATE index_jobs
              SET total_files = total_files + ?2,
                  updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1 AND state IN ('discovering', 'parsing')",
            params![job_id, i64::try_from(inserted_total).unwrap_or(i64::MAX)],
        )?;
        // The parser can consume this batch immediately. The recovery flag
        // remains false until the walk has fully completed.
        transaction.execute(
            "UPDATE index_jobs
             SET state = 'parsing',
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1 AND state = 'discovering'",
            [job_id],
        )?;
    }
    transaction.commit()?;
    Ok(inserted_total)
}

fn upsert_discovered_metadata_transaction(
    transaction: &rusqlite::Transaction<'_>,
    folder_id: &str,
    candidate: &FileCandidate,
) -> Result<(), IndexingError> {
    let canonical_path = candidate.canonical_path.to_string_lossy().into_owned();
    let name = file_name(&candidate.canonical_path);
    let extension = candidate
        .canonical_path
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let modified_at = modified_at_string(candidate.modified_at);
    transaction.execute(
        "INSERT INTO documents
         (id, folder_id, canonical_path, file_name, extension, size_bytes,
          modified_at, parse_state, parse_error_code, parse_attempt_token)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', NULL, NULL)
         ON CONFLICT(canonical_path) DO UPDATE SET
           folder_id = excluded.folder_id,
           file_name = excluded.file_name,
           extension = excluded.extension,
           size_bytes = excluded.size_bytes,
           modified_at = excluded.modified_at,
           parse_state = CASE
             WHEN documents.size_bytes = excluded.size_bytes
              AND documents.modified_at = excluded.modified_at
              AND documents.parse_state IN ('parsed', 'metadata_only')
             THEN documents.parse_state
             ELSE 'pending'
           END,
           parse_error_code = CASE
             WHEN documents.size_bytes = excluded.size_bytes
              AND documents.modified_at = excluded.modified_at
              AND documents.parse_state IN ('parsed', 'metadata_only')
             THEN documents.parse_error_code
             ELSE NULL
           END,
           parse_attempt_token = NULL
         WHERE documents.parse_attempt_token IS NULL",
        params![
            random_id(),
            folder_id,
            canonical_path,
            name,
            extension,
            i64::try_from(candidate.size_bytes).unwrap_or(i64::MAX),
            modified_at,
        ],
    )?;

    let document_id: String = transaction.query_row(
        "SELECT id FROM documents WHERE canonical_path = ?1",
        [&canonical_path],
        |row| row.get(0),
    )?;
    // Keep the last committed body/FTS row while a changed file is pending.
    // This makes background indexing non-destructive: a transient parse
    // failure or an in-flight rename must not make an already searchable
    // document disappear. The parser replaces the row atomically after a
    // successful extraction. New files still receive a filename-only row so
    // filename/path searches work immediately during discovery.
    transaction.execute(
        "INSERT INTO document_fts (document_id, file_name, title, body)
         SELECT id, file_name, '', '' FROM documents
         WHERE id = ?1
           AND NOT EXISTS (
             SELECT 1 FROM document_fts WHERE document_id = ?1
           )",
        [&document_id],
    )?;
    Ok(())
}

fn reset_in_flight_candidates(database: &Database, job_id: &str) {
    let _ = database.connection().execute(
        "UPDATE index_job_files SET state = 'queued'
         WHERE job_id = ?1 AND state = 'in_flight'",
        [job_id],
    );
}

fn send_pending_candidates(
    database: &Database,
    job_id: &str,
    sender: mpsc::Sender<PersistedCandidate>,
) -> Result<(), IndexingError> {
    loop {
        let (batch, discovery_complete) = {
            let mut connection = database.connection();
            let transaction = connection.transaction()?;
            let (state, discovery_complete): (String, bool) = transaction.query_row(
                "SELECT j.state, r.discovery_complete
                 FROM index_jobs j
                 JOIN index_job_recovery r ON r.job_id = j.id
                 WHERE j.id = ?1",
                [job_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if state != "parsing" {
                transaction.commit()?;
                return Ok(());
            }
            let mut statement = transaction.prepare(
                "SELECT canonical_path, relative_path, size_bytes, modified_at, metadata_only
                 FROM index_job_files
                 WHERE job_id = ?1 AND state = 'queued'
                 ORDER BY relative_path, canonical_path
                 LIMIT ?2",
            )?;
            let rows = statement
                .query_map(
                    params![job_id, i64::try_from(PIPELINE_CAPACITY).unwrap_or(16)],
                    |row| {
                        Ok(PersistedCandidate {
                            job_id: job_id.to_owned(),
                            canonical_path: PathBuf::from(row.get::<_, String>(0)?),
                            relative_path: row.get(1)?,
                            size_bytes: row.get(2)?,
                            modified_at: row.get(3)?,
                            metadata_only: row.get(4)?,
                        })
                    },
                )?
                .collect::<Result<Vec<_>, _>>()?;
            drop(statement);
            for candidate in &rows {
                transaction.execute(
                    "UPDATE index_job_files SET state = 'in_flight'
                     WHERE job_id = ?1 AND canonical_path = ?2 AND state = 'queued'",
                    params![job_id, candidate.canonical_path.to_string_lossy()],
                )?;
            }
            transaction.commit()?;
            (rows, discovery_complete)
        };
        if batch.is_empty() {
            if discovery_complete {
                return Ok(());
            }
            // Discovery is still producing metadata rows. Keep the producer
            // alive instead of exiting before later candidates arrive.
            std::thread::sleep(Duration::from_millis(25));
            continue;
        }
        for candidate in batch {
            if sender.blocking_send(candidate).is_err() {
                return Ok(());
            }
        }
    }
}

fn upsert_metadata_transaction(
    connection: &rusqlite::Connection,
    folder_id: &str,
    candidate: &PersistedCandidate,
    parse_state: &str,
    error_code: Option<&str>,
    attempt_token: Option<&str>,
    allow_attempt_takeover: bool,
) -> Result<ParseAttemptMutation, IndexingError> {
    let canonical_path = candidate.canonical_path.to_string_lossy().into_owned();
    let name = file_name(&candidate.canonical_path);
    let extension = candidate
        .canonical_path
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let changed = connection.execute(
        "INSERT INTO documents
         (id, folder_id, canonical_path, file_name, extension, size_bytes,
           modified_at, parse_state, parse_error_code, parse_attempt_token)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(canonical_path) DO UPDATE SET
           folder_id = excluded.folder_id,
           file_name = excluded.file_name,
           extension = excluded.extension,
           size_bytes = excluded.size_bytes,
           modified_at = excluded.modified_at,
           parse_state = excluded.parse_state,
           parse_error_code = excluded.parse_error_code,
           parse_attempt_token = excluded.parse_attempt_token
         WHERE ?11 OR documents.parse_attempt_token IS NULL",
        params![
            random_id(),
            folder_id,
            canonical_path,
            name,
            extension,
            candidate.size_bytes,
            candidate.modified_at,
            parse_state,
            error_code,
            attempt_token,
            allow_attempt_takeover,
        ],
    )?;
    Ok(if changed == 1 {
        ParseAttemptMutation::Applied
    } else {
        ParseAttemptMutation::Stale
    })
}

fn ensure_job_parsing(
    transaction: &rusqlite::Transaction<'_>,
    job_id: &str,
) -> Result<(), IndexingError> {
    let parsing: bool = transaction.query_row(
        "SELECT state = 'parsing' FROM index_jobs WHERE id = ?1",
        [job_id],
        |row| row.get(0),
    )?;
    if parsing {
        Ok(())
    } else {
        Err(IndexingError::StateChanged)
    }
}

fn folder_id_for_job(
    transaction: &rusqlite::Transaction<'_>,
    job_id: &str,
) -> Result<String, IndexingError> {
    Ok(transaction.query_row(
        "SELECT folder_id FROM index_jobs WHERE id = ?1",
        [job_id],
        |row| row.get(0),
    )?)
}

fn advance_file_transaction(
    transaction: &rusqlite::Transaction<'_>,
    job_id: &str,
    candidate: &PersistedCandidate,
) -> Result<(), rusqlite::Error> {
    let changed = transaction.execute(
        "UPDATE index_job_files SET state = 'completed'
         WHERE job_id = ?1 AND canonical_path = ?2 AND state != 'completed'",
        params![job_id, candidate.canonical_path.to_string_lossy()],
    )?;
    if changed != 1 {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    transaction.execute(
        "UPDATE index_jobs
         SET completed_files = (
               SELECT COUNT(*) FROM index_job_files
               WHERE job_id = ?1 AND state = 'completed'
             ),
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
         WHERE id = ?1",
        [job_id],
    )?;
    Ok(())
}

fn is_parse_attempt_token_collision(error: &IndexingError) -> bool {
    matches!(
        error,
        IndexingError::Database(rusqlite::Error::SqliteFailure(code, Some(message)))
            if code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
                && message == "UNIQUE constraint failed: documents.parse_attempt_token"
    )
}

fn should_track_ocr(path: &Path, settings: &RuntimeIndexSettings) -> bool {
    if !settings.ocr_enabled {
        return false;
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "pdf" | "jpg" | "jpeg" | "png" | "webp" | "bmp" | "tif" | "tiff"
    )
}

fn candidate_requires_extraction(path: &Path, settings: &RuntimeIndexSettings) -> bool {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(
        extension.as_str(),
        "txt" | "md" | "markdown" | "hwp" | "hwpx" | "hml" | "hwpml" | "pdf" | "xlsx" | "docx"
    ) {
        return true;
    }
    settings.ocr_enabled
        && matches!(
            extension.as_str(),
            "jpg" | "jpeg" | "png" | "webp" | "bmp" | "tif" | "tiff"
        )
}

fn document_was_ocr(document: &ParsedDocument) -> bool {
    document
        .metadata
        .get("engine")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|engine| engine.eq_ignore_ascii_case("paddleocr"))
}

fn extract_document(
    parser: Arc<dyn DocumentParser>,
    ocr: Option<Arc<dyn DocumentOcr>>,
    path: &Path,
    settings: &RuntimeIndexSettings,
    cancelled: Arc<AtomicBool>,
) -> Result<ParsedDocument, ExtractionError> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let initial_decision = decide_ocr(
        &extension,
        "",
        settings.ocr_enabled,
        settings.math_ocr_enabled,
        settings.math_ocr_enabled && extension == "pdf",
    );
    if matches!(
        initial_decision,
        OcrDecision::TextOcr | OcrDecision::MathOcr
    ) && extension != "pdf"
    {
        return run_ocr(
            ocr,
            path,
            initial_decision,
            settings.max_file_size_bytes,
            cancelled,
        );
    }

    match parser.parse(path, settings.max_file_size_bytes) {
        Ok(document) => {
            let decision = decide_ocr(
                &extension,
                &document.plain_text,
                settings.ocr_enabled,
                settings.math_ocr_enabled,
                settings.math_ocr_enabled && extension == "pdf",
            );
            match decision {
                OcrDecision::TextOcr | OcrDecision::MathOcr => {
                    run_ocr(ocr, path, decision, settings.max_file_size_bytes, cancelled)
                }
                _ => Ok(document),
            }
        }
        Err(ParserError::Protocol {
            code: ParseErrorCode::ImageBasedPdf,
            ..
        }) if settings.ocr_enabled => {
            let decision = if settings.math_ocr_enabled {
                OcrDecision::MathOcr
            } else {
                OcrDecision::TextOcr
            };
            run_ocr(ocr, path, decision, settings.max_file_size_bytes, cancelled)
        }
        Err(error) => Err(ExtractionError::Parser(error)),
    }
}

fn run_ocr(
    ocr: Option<Arc<dyn DocumentOcr>>,
    path: &Path,
    decision: OcrDecision,
    max_bytes: u64,
    cancelled: Arc<AtomicBool>,
) -> Result<ParsedDocument, ExtractionError> {
    let ocr = ocr.ok_or(ExtractionError::OcrUnavailable)?;
    let mode = if decision == OcrDecision::MathOcr {
        OcrMode::Math
    } else {
        OcrMode::Text
    };
    ocr.recognize(path, mode, max_bytes, cancelled)
        .map_err(ExtractionError::Ocr)
}

fn extraction_failure(error: &ExtractionError) -> (&'static str, String) {
    match error {
        ExtractionError::Parser(error) => parser_failure(error),
        ExtractionError::Ocr(error) => match error {
            OcrError::Protocol { code, message } => (ocr_error_code(code), message.clone()),
            OcrError::Start => ("OCR_START", error.to_string()),
            OcrError::Io => ("OCR_IO", error.to_string()),
            OcrError::UnexpectedExit => ("OCR_EXIT", error.to_string()),
            OcrError::InvalidResponse => ("OCR_INVALID_RESPONSE", error.to_string()),
            OcrError::Timeout => ("OCR_TIMEOUT", error.to_string()),
            OcrError::Cancelled => ("OCR_CANCELLED", error.to_string()),
        },
        ExtractionError::OcrUnavailable => (
            "OCR_UNAVAILABLE",
            "Local OCR is enabled but its bundled engine is unavailable".into(),
        ),
    }
}

fn ocr_error_code(code: &crate::ocr::OcrErrorCode) -> &'static str {
    use crate::ocr::OcrErrorCode;
    match code {
        OcrErrorCode::InvalidRequest => "OCR_INVALID_REQUEST",
        OcrErrorCode::FileUnavailable => "OCR_FILE_UNAVAILABLE",
        OcrErrorCode::Unsupported => "OCR_UNSUPPORTED",
        OcrErrorCode::TooLarge => "OCR_TOO_LARGE",
        OcrErrorCode::ModelMissing => "OCR_MODEL_MISSING",
        OcrErrorCode::InvalidEngineResult => "OCR_INVALID_ENGINE_RESULT",
        OcrErrorCode::Internal => "OCR_INTERNAL",
    }
}

fn parser_failure(error: &ParserError) -> (&'static str, String) {
    match error {
        ParserError::Protocol { code, message } => (parse_error_code(code), message.clone()),
        ParserError::Start => ("PARSER_START", error.to_string()),
        ParserError::Io => ("PARSER_IO", error.to_string()),
        ParserError::UnexpectedExit => ("PARSER_EXIT", error.to_string()),
        ParserError::InvalidResponse => ("PARSER_INVALID_RESPONSE", error.to_string()),
        ParserError::Timeout => ("TIMEOUT", error.to_string()),
    }
}

fn parse_error_code(code: &ParseErrorCode) -> &'static str {
    match code {
        ParseErrorCode::InvalidRequest => "INVALID_REQUEST",
        ParseErrorCode::Unsupported => "UNSUPPORTED",
        ParseErrorCode::Encrypted => "ENCRYPTED",
        ParseErrorCode::Damaged => "DAMAGED",
        ParseErrorCode::Timeout => "TIMEOUT",
        ParseErrorCode::TooLarge => "TOO_LARGE",
        ParseErrorCode::ImageBasedPdf => "IMAGE_BASED_PDF",
        ParseErrorCode::Internal => "INTERNAL",
    }
}

fn path_is_metadata_only(path: &Path) -> Result<bool, IndexingError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(source) => {
            return Err(IndexingError::PathUnavailable {
                path: path.to_string_lossy().into_owned(),
                source,
            })
        }
    };
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        Ok(crate::folders::discovery::is_metadata_only_file_attributes(
            metadata.file_attributes(),
        ))
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        Ok(false)
    }
}

fn lexically_within(root: &Path, path: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    parent
        .canonicalize()
        .map(|parent| parent.starts_with(root))
        .unwrap_or(false)
}

trait EventPathProvider {
    fn canonicalize_parent(&self, path: &Path) -> std::io::Result<PathBuf>;
}

struct OsEventPathProvider;

impl EventPathProvider for OsEventPathProvider {
    fn canonicalize_parent(&self, path: &Path) -> std::io::Result<PathBuf> {
        path.canonicalize()
    }
}

fn trusted_event_identity(
    canonical_root: &Path,
    event_path: &Path,
    provider: &dyn EventPathProvider,
) -> Result<Option<PathBuf>, IndexingError> {
    let (Some(parent), Some(file_name)) = (event_path.parent(), event_path.file_name()) else {
        return Ok(None);
    };
    let canonical_parent = match provider.canonicalize_parent(parent) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(IndexingError::PathUnavailable {
                path: parent.to_string_lossy().into_owned(),
                source: error,
            })
        }
    };
    Ok(canonical_parent
        .starts_with(canonical_root)
        .then(|| canonical_parent.join(file_name)))
}

fn modified_at_string(time: Option<SystemTime>) -> String {
    time.and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos().to_string())
        .unwrap_or_default()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn random_id() -> String {
    let bytes = rand::rng().random::<[u8; 16]>();
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Debug, Error)]
pub enum IndexingError {
    #[error("registered folder was not found: {0}")]
    FolderNotFound(String),
    #[error("indexing job was not found: {0}")]
    JobNotFound(String),
    #[error("invalid persisted indexing state: {0}")]
    InvalidState(String),
    #[error("cannot transition indexing job from {from:?} to {to:?}")]
    Transition { from: JobState, to: JobState },
    #[error("candidate resolved outside its registered root")]
    PathOutsideRoot,
    #[error("metadata-only placeholder cannot be parsed")]
    MetadataOnly,
    #[error("path is unavailable: {path}")]
    PathUnavailable {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("document metadata disappeared before content commit")]
    MissingDocument,
    #[error("indexing worker stopped unexpectedly")]
    WorkerStopped,
    #[error("indexing job already has an active worker: {0}")]
    AlreadyRunning(String),
    #[error("indexing state changed while work was in flight")]
    StateChanged,
    #[error("indexing worker failed: {0}")]
    WorkerFailure(String),
    #[error("reconciliation was cancelled")]
    ReconciliationCancelled,
    #[error("runtime indexing settings are unavailable")]
    RuntimeSettingsUnavailable,
    #[error("folder discovery failed")]
    Discovery(#[from] crate::folders::discovery::DiscoveryError),
    #[error("indexing database operation failed")]
    Database(#[from] rusqlite::Error),
    #[error("indexing serialization failed")]
    Serialization(#[from] serde_json::Error),
    #[error("file metadata check failed")]
    Io(#[from] std::io::Error),
}

impl IndexingError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::FolderNotFound(_) => "INDEX_FOLDER_NOT_FOUND",
            Self::JobNotFound(_) => "INDEX_JOB_NOT_FOUND",
            Self::InvalidState(_) | Self::Transition { .. } => "INDEX_INVALID_STATE",
            Self::PathOutsideRoot | Self::MetadataOnly => "INDEX_PATH_NOT_TRUSTED",
            Self::PathUnavailable { .. } | Self::Io(_) => "INDEX_PATH_UNAVAILABLE",
            Self::MissingDocument => "INDEX_DOCUMENT_MISSING",
            Self::WorkerStopped => "INDEX_WORKER_STOPPED",
            Self::AlreadyRunning(_) => "INDEX_ALREADY_RUNNING",
            Self::StateChanged => "INDEX_STATE_CHANGED",
            Self::WorkerFailure(_) => "INDEX_WORKER_FAILED",
            Self::ReconciliationCancelled => "INDEX_RECONCILIATION_CANCELLED",
            Self::RuntimeSettingsUnavailable => "INDEX_RUNTIME_SETTINGS_UNAVAILABLE",
            Self::Discovery(_) => "INDEX_DISCOVERY_FAILED",
            Self::Database(_) => "INDEX_DATABASE_ERROR",
            Self::Serialization(_) => "INDEX_SERIALIZATION_ERROR",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use super::{
        advance_file_transaction, candidate_requires_extraction, trusted_event_identity,
        DocumentParser, EventPathProvider, IndexCoordinator, JobRuntime, JobState,
        PersistedCandidate, ReconciliationScratch, RuntimeIndexSettings,
    };
    use crate::domain::models::IndexStatus;
    use crate::folders::repository::FolderRepository;
    use crate::infrastructure::database::Database;
    use crate::infrastructure::secure_key::SecretKey;
    use crate::parsing::{ParsedDocument, ParserError};
    use zeroize::Zeroizing;

    #[test]
    fn mixed_file_candidates_only_extract_supported_content() {
        let mut settings = RuntimeIndexSettings {
            max_file_size_bytes: 10_000,
            excluded_path_patterns: vec![],
            indexing_intensity: "balanced".into(),
            ocr_enabled: false,
            math_ocr_enabled: false,
        };
        assert!(candidate_requires_extraction(
            Path::new("report.pdf"),
            &settings
        ));
        assert!(candidate_requires_extraction(
            Path::new("notes.hwp"),
            &settings
        ));
        assert!(candidate_requires_extraction(
            Path::new("sheet.xlsx"),
            &settings
        ));
        assert!(!candidate_requires_extraction(
            Path::new("setup.exe"),
            &settings
        ));
        assert!(!candidate_requires_extraction(
            Path::new("scan.png"),
            &settings
        ));

        settings.ocr_enabled = true;
        assert!(candidate_requires_extraction(
            Path::new("scan.png"),
            &settings
        ));
        assert!(!candidate_requires_extraction(
            Path::new("archive.zip"),
            &settings
        ));
    }

    struct TestParser;

    impl DocumentParser for TestParser {
        fn parse(&self, path: &Path, _max_bytes: u64) -> Result<ParsedDocument, ParserError> {
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

    struct RecordingParser {
        max_bytes: Arc<std::sync::Mutex<Vec<u64>>>,
    }

    impl DocumentParser for RecordingParser {
        fn parse(&self, path: &Path, max_bytes: u64) -> Result<ParsedDocument, ParserError> {
            self.max_bytes.lock().unwrap().push(max_bytes);
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

    struct ParentOnlyProvider {
        expected_parent: PathBuf,
    }

    impl EventPathProvider for ParentOnlyProvider {
        fn canonicalize_parent(&self, path: &Path) -> std::io::Result<PathBuf> {
            assert_eq!(path, self.expected_parent);
            Ok(path.to_path_buf())
        }
    }

    #[test]
    fn watcher_identity_canonicalizes_only_the_parent_not_the_candidate_file() {
        let root = PathBuf::from("C:\\registered");
        let event_path = root.join("cloud-placeholder.pdf");
        let provider = ParentOnlyProvider {
            expected_parent: root.clone(),
        };

        let identity = trusted_event_identity(&root, &event_path, &provider).unwrap();

        assert_eq!(identity, Some(event_path));
    }

    #[tokio::test]
    async fn stale_status_reap_cannot_remove_or_orphan_a_resumed_worker() {
        let runtimes = Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));
        let stale = Arc::new(JobRuntime::new());
        let replacement = Arc::new(JobRuntime::new());
        let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let worker_completed = Arc::clone(&completed);
        *replacement.handle.lock().await = Some(tokio::spawn(async move {
            worker_completed.fetch_add(1, Ordering::AcqRel);
        }));
        runtimes
            .lock()
            .await
            .insert("job".to_owned(), Arc::clone(&replacement));

        IndexCoordinator::remove_runtime_if_same(&runtimes, "job", &stale).await;

        let retained = runtimes.lock().await.get("job").cloned().unwrap();
        assert!(Arc::ptr_eq(&retained, &replacement));
        let handle = retained.handle.lock().await.take().unwrap();
        handle.await.unwrap();
        assert_eq!(completed.load(Ordering::Acquire), 1);
    }

    #[test]
    fn duplicate_file_completion_is_rejected_before_progress_update() {
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE index_jobs (
                   id TEXT PRIMARY KEY,
                   completed_files INTEGER NOT NULL,
                   updated_at TEXT
                 );
                 CREATE TABLE index_job_files (
                   job_id TEXT NOT NULL,
                   canonical_path TEXT NOT NULL,
                   state TEXT NOT NULL,
                   PRIMARY KEY (job_id, canonical_path)
                 );
                 INSERT INTO index_jobs (id, completed_files) VALUES ('job', 0);
                 INSERT INTO index_job_files (job_id, canonical_path, state)
                 VALUES ('job', 'C:\\docs\\one.txt', 'queued');",
            )
            .unwrap();
        let candidate = PersistedCandidate {
            job_id: "job".into(),
            canonical_path: PathBuf::from(r"C:\docs\one.txt"),
            relative_path: "one.txt".into(),
            size_bytes: 1,
            modified_at: "1".into(),
            metadata_only: false,
        };

        let transaction = connection.transaction().unwrap();
        advance_file_transaction(&transaction, "job", &candidate).unwrap();
        transaction.commit().unwrap();
        let transaction = connection.transaction().unwrap();
        assert!(advance_file_transaction(&transaction, "job", &candidate).is_err());
        transaction.rollback().unwrap();

        let completed: i64 = connection
            .query_row(
                "SELECT completed_files FROM index_jobs WHERE id = 'job'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(completed, 1);
    }

    #[tokio::test]
    async fn completed_spawned_and_direct_jobs_leave_no_registered_runtime() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("one.txt"), "one").unwrap();
        let key = SecretKey::from_bytes(Zeroizing::new([33_u8; 32]));
        let database = Arc::new(Database::open(&temp.path().join("index.db"), &key).unwrap());
        database.migrate().unwrap();
        let folder = FolderRepository::new(Arc::clone(&database))
            .register(&root)
            .unwrap();
        let coordinator =
            IndexCoordinator::with_parser(Arc::clone(&database), Arc::new(TestParser), 1024 * 1024);

        let job_id = coordinator.start(&folder.id).await.unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if coordinator.job_state(&job_id).unwrap() == JobState::Completed {
                break;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        tokio::task::yield_now().await;
        assert!(coordinator.runtimes.lock().await.is_empty());

        for index in 0..20 {
            fs::write(
                root.join("one.txt"),
                format!("changed body {index} {}", "x".repeat(index)),
            )
            .unwrap();
            coordinator
                .reindex_discovered_path(&folder.id, &root.join("one.txt"))
                .await
                .unwrap();
            coordinator.reconcile(&folder.id).await.unwrap();
            assert!(coordinator.runtimes.lock().await.is_empty());
        }
    }

    #[tokio::test]
    async fn watcher_reindex_status_is_silent_but_manual_status_is_visible() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("one.txt"), "one").unwrap();
        let key = SecretKey::from_bytes(Zeroizing::new([34_u8; 32]));
        let database = Arc::new(Database::open(&temp.path().join("index.db"), &key).unwrap());
        database.migrate().unwrap();
        let folder = FolderRepository::new(Arc::clone(&database))
            .register(&root)
            .unwrap();
        let statuses = Arc::new(std::sync::Mutex::new(Vec::<IndexStatus>::new()));
        let observed: Arc<std::sync::Mutex<Vec<IndexStatus>>> = Arc::clone(&statuses);
        let coordinator = IndexCoordinator::with_parser_and_sink(
            Arc::clone(&database),
            Arc::new(TestParser),
            1024 * 1024,
            Some(Arc::new(move |status| {
                observed.lock().unwrap().push(status);
                Ok(())
            })),
        );

        let manual_job = coordinator.start(&folder.id).await.unwrap();
        wait_for_completed(&coordinator, &manual_job).await;
        assert!(statuses
            .lock()
            .unwrap()
            .iter()
            .any(|status| status.job_id == manual_job && !status.silent));

        fs::write(root.join("one.txt"), "changed").unwrap();
        coordinator
            .reindex_discovered_path(&folder.id, &root.join("one.txt"))
            .await
            .unwrap();
        assert!(statuses
            .lock()
            .unwrap()
            .iter()
            .any(|status| status.job_id != manual_job && status.silent));
    }

    #[tokio::test]
    async fn saved_runtime_settings_change_size_exclusions_and_work_intensity() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("included.txt"), "included").unwrap();
        fs::write(root.join("alternate.md"), "alternate").unwrap();
        let key = SecretKey::from_bytes(Zeroizing::new([92_u8; 32]));
        let database = Arc::new(Database::open(&temp.path().join("index.db"), &key).unwrap());
        database.migrate().unwrap();
        let folder = FolderRepository::new(Arc::clone(&database))
            .register(&root)
            .unwrap();
        let observed = Arc::new(std::sync::Mutex::new(Vec::new()));
        let coordinator = IndexCoordinator::with_parser(
            Arc::clone(&database),
            Arc::new(RecordingParser {
                max_bytes: Arc::clone(&observed),
            }),
            999,
        );
        coordinator
            .apply_runtime_settings(&crate::domain::models::AppSettings {
                max_file_size_bytes: 123,
                excluded_path_patterns: vec!["*.md".into()],
                indexing_intensity: "high".into(),
                ..Default::default()
            })
            .unwrap();

        let first = coordinator.start(&folder.id).await.unwrap();
        wait_for_completed(&coordinator, &first).await;
        assert_eq!(*observed.lock().unwrap(), [123]);

        coordinator
            .apply_runtime_settings(&crate::domain::models::AppSettings {
                max_file_size_bytes: 456,
                excluded_path_patterns: vec!["*.txt".into()],
                indexing_intensity: "low".into(),
                ..Default::default()
            })
            .unwrap();
        let started = Instant::now();
        let second = coordinator.start(&folder.id).await.unwrap();
        wait_for_completed(&coordinator, &second).await;

        assert!(
            started.elapsed() >= Duration::from_millis(15),
            "low intensity must yield CPU time before parser work"
        );
        assert_eq!(*observed.lock().unwrap(), [123, 456]);
    }

    async fn wait_for_completed(coordinator: &IndexCoordinator, job_id: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if coordinator.job_state(job_id).unwrap() == JobState::Completed {
                return;
            }
            assert!(Instant::now() < deadline, "indexing job did not complete");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[test]
    fn concurrent_reconciliation_runs_keep_independent_seen_sets() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        fs::create_dir(&root).unwrap();
        let key = SecretKey::from_bytes(Zeroizing::new([44_u8; 32]));
        let database = Arc::new(Database::open(&temp.path().join("index.db"), &key).unwrap());
        database.migrate().unwrap();
        let folder = FolderRepository::new(Arc::clone(&database))
            .register(&root)
            .unwrap();
        for index in 1..=100 {
            database
                .connection()
                .execute(
                    "INSERT INTO documents
                     (id, folder_id, canonical_path, file_name, extension,
                      size_bytes, modified_at, parse_state)
                     VALUES (?1, ?2, ?3, ?4, 'txt', 1, '1', 'indexed')",
                    rusqlite::params![
                        format!("doc-{index}"),
                        folder.id,
                        format!(r"C:\root\{index:03}.txt"),
                        format!("{index:03}.txt"),
                    ],
                )
                .unwrap();
        }
        let first = ReconciliationScratch::begin(Arc::clone(&database), &folder.id).unwrap();
        for index in 1..=50 {
            first
                .record(Path::new(&format!(r"C:\root\{index:03}.txt")))
                .unwrap();
        }
        let second = ReconciliationScratch::begin(Arc::clone(&database), &folder.id).unwrap();
        for index in 1..=10 {
            second
                .record(Path::new(&format!(r"C:\root\{index:03}.txt")))
                .unwrap();
        }
        for index in 51..=100 {
            first
                .record(Path::new(&format!(r"C:\root\{index:03}.txt")))
                .unwrap();
        }

        assert_eq!(first.first_missing().unwrap(), None);
        assert_eq!(
            second.first_missing().unwrap(),
            Some(r"C:\root\011.txt".to_owned())
        );
    }

    #[test]
    fn reconciliation_scratch_cleans_up_when_scope_fails() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        fs::create_dir(&root).unwrap();
        let key = SecretKey::from_bytes(Zeroizing::new([45_u8; 32]));
        let database = Arc::new(Database::open(&temp.path().join("index.db"), &key).unwrap());
        database.migrate().unwrap();
        let folder = FolderRepository::new(Arc::clone(&database))
            .register(&root)
            .unwrap();
        {
            let scratch = ReconciliationScratch::begin(Arc::clone(&database), &folder.id).unwrap();
            scratch.record(Path::new(r"C:\root\one.txt")).unwrap();
        }

        let runs: i64 = database
            .connection()
            .query_row("SELECT COUNT(*) FROM reconciliation_runs", [], |row| {
                row.get(0)
            })
            .unwrap();
        let seen: i64 = database
            .connection()
            .query_row("SELECT COUNT(*) FROM reconciliation_seen_v2", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!((runs, seen), (0, 0));
    }
}
