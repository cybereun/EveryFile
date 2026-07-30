use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rand::RngExt;
use rusqlite::{params, OptionalExtension};
use thiserror::Error;
use tokio::sync::{mpsc, Notify};
use tokio::task::JoinHandle;

use crate::domain::models::{FolderRecord, IndexFailure, IndexStatus, JobState};
use crate::folders::discovery::{discover, DiscoveryOptions, FileCandidate};
use crate::infrastructure::database::Database;
use crate::parsing::{ParseErrorCode, ParsedDocument, ParserClient, ParserError};

const PIPELINE_CAPACITY: usize = 16;

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

pub trait DiscoveryProbe: Send + Sync + 'static {
    fn candidate_persisted(&self, buffered_candidates: usize);
}

struct NoopDiscoveryProbe;

impl DiscoveryProbe for NoopDiscoveryProbe {
    fn candidate_persisted(&self, _buffered_candidates: usize) {}
}

impl DocumentParser for ParserClient {
    fn parse(&self, path: &Path, max_bytes: u64) -> Result<ParsedDocument, ParserError> {
        ParserClient::parse(self, path, max_bytes)
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
    max_file_size_bytes: u64,
    limiter: ActivityLimiter,
    runtimes: Arc<tokio::sync::Mutex<HashMap<JobId, Arc<JobRuntime>>>>,
    status_sink: Option<Arc<StatusSink>>,
    discovery_probe: Arc<dyn DiscoveryProbe>,
}

struct JobRuntime {
    stop: AtomicBool,
    gate: tokio::sync::Mutex<()>,
    handle: tokio::sync::Mutex<Option<JoinHandle<()>>>,
}

impl JobRuntime {
    fn new() -> Self {
        Self {
            stop: AtomicBool::new(false),
            gate: tokio::sync::Mutex::new(()),
            handle: tokio::sync::Mutex::new(None),
        }
    }
}

impl IndexCoordinator {
    pub fn with_parser<P>(database: Arc<Database>, parser: Arc<P>, max_file_size_bytes: u64) -> Self
    where
        P: DocumentParser,
    {
        Self::with_parser_probe_and_sink(
            database,
            parser,
            max_file_size_bytes,
            Arc::new(NoopDiscoveryProbe),
            None,
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
            max_file_size_bytes,
            Arc::new(NoopDiscoveryProbe),
            status_sink,
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
            max_file_size_bytes,
            discovery_probe,
            None,
        )
    }

    fn with_parser_probe_and_sink<P>(
        database: Arc<Database>,
        parser: Arc<P>,
        max_file_size_bytes: u64,
        discovery_probe: Arc<dyn DiscoveryProbe>,
        status_sink: Option<Arc<StatusSink>>,
    ) -> Self
    where
        P: DocumentParser,
    {
        Self {
            database,
            parser,
            max_file_size_bytes,
            limiter: ActivityLimiter::default(),
            runtimes: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            status_sink,
            discovery_probe,
        }
    }

    pub fn activity_limiter(&self) -> ActivityLimiter {
        self.limiter.clone()
    }

    pub async fn start(&self, folder_id: &str) -> Result<JobId, IndexingError> {
        self.registered_folder(folder_id)?;
        let job_id = random_id();
        {
            let mut connection = self.database.connection();
            let transaction = connection.transaction()?;
            transaction.execute(
                "INSERT INTO index_jobs
                 (id, folder_id, state, completed_files, total_files, last_path, updated_at)
                 VALUES (?1, ?2, ?3, 0, 0, NULL, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
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
        let gate = runtime.gate.lock().await;
        self.cas_active_state(job_id, JobState::Paused)?;
        runtime.stop.store(true, Ordering::Release);
        drop(gate);
        self.await_worker(&runtime).await;
        self.emit_status(job_id);
        Ok(())
    }

    pub async fn resume(&self, job_id: &str) -> Result<(), IndexingError> {
        let runtime = self.runtime(job_id).await;
        let gate = runtime.gate.lock().await;
        self.reap_runtime_handle(&runtime).await;
        let target = if self.discovery_complete(job_id)? {
            JobState::Parsing
        } else {
            JobState::Discovering
        };
        self.cas_state(job_id, JobState::Paused, target)?;
        runtime.stop.store(false, Ordering::Release);
        drop(gate);
        self.spawn_job(job_id.to_owned()).await?;
        self.emit_status(job_id);
        Ok(())
    }

    pub async fn cancel(&self, job_id: &str) -> Result<(), IndexingError> {
        let runtime = self.runtime(job_id).await;
        let gate = runtime.gate.lock().await;
        self.cas_cancellable_state(job_id)?;
        runtime.stop.store(true, Ordering::Release);
        drop(gate);
        self.await_worker(&runtime).await;
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

    async fn await_worker(&self, runtime: &Arc<JobRuntime>) {
        let mut handle = runtime.handle.lock().await;
        if let Some(handle) = handle.take() {
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
        let candidate = tokio::task::spawn_blocking(move || {
            let stream = discover(&folder, DiscoveryOptions::default())?;
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
            self.create_single_candidate_job(&job_id, folder_id, candidate)?;
            let runtime = self.runtime(&job_id).await;
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
                "SELECT size_bytes, modified_at FROM documents
                 WHERE folder_id = ?1 AND canonical_path = ?2",
                params![folder_id, candidate.canonical_path.to_string_lossy()],
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
        let Some(old) = self.stored_path_for_event(folder_id, from)? else {
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

        let matched = {
            let mut connection = self.database.connection();
            let transaction = connection.transaction()?;
            let identity: Option<(String, i64, String)> = transaction
                .query_row(
                    "SELECT id, size_bytes, modified_at FROM documents
                     WHERE folder_id = ?1 AND canonical_path = ?2",
                    params![folder_id, old],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            let matched = identity.as_ref().is_some_and(|(_, size, modified)| {
                (*size, modified.as_str()) == (new_size, new_modified.as_str())
            });
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
                     SET canonical_path = ?3, file_name = ?4, extension = ?5
                     WHERE folder_id = ?1 AND canonical_path = ?2",
                    params![folder_id, old, new, new_name, new_extension],
                )?;
                transaction.execute(
                    "UPDATE document_fts SET file_name = ?2 WHERE document_id = ?1",
                    params![document_id, new_name],
                )?;
            }
            transaction.commit()?;
            matched
        };
        if !matched {
            self.delete_document(folder_id, from)?;
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
        let folder = self.registered_folder(folder_id)?;
        let database = Arc::clone(&self.database);
        let reconcile_folder_id = folder_id.to_owned();
        tokio::task::spawn_blocking(move || {
            database.connection().execute(
                "DELETE FROM reconciliation_seen WHERE folder_id = ?1",
                [&reconcile_folder_id],
            )?;
            Ok::<(), IndexingError>(())
        })
        .await
        .map_err(|_| IndexingError::WorkerStopped)??;

        let (sender, mut receiver) =
            mpsc::channel::<Result<FileCandidate, IndexingError>>(PIPELINE_CAPACITY);
        let database = Arc::clone(&self.database);
        let producer_folder_id = folder_id.to_owned();
        let producer = tokio::task::spawn_blocking(move || {
            let stream = match discover(&folder, DiscoveryOptions::default()) {
                Ok(stream) => stream,
                Err(error) => {
                    let _ = sender.blocking_send(Err(error.into()));
                    return;
                }
            };
            for candidate in stream {
                let path = candidate.canonical_path.to_string_lossy().into_owned();
                let inserted = database.connection().execute(
                    "INSERT OR IGNORE INTO reconciliation_seen
                     (folder_id, canonical_path) VALUES (?1, ?2)",
                    params![producer_folder_id, path],
                );
                if let Err(error) = inserted {
                    let _ = sender.blocking_send(Err(error.into()));
                    return;
                }
                if sender.blocking_send(Ok(candidate)).is_err() {
                    return;
                }
            }
        });

        while let Some(candidate) = receiver.recv().await {
            match candidate {
                Ok(candidate) => {
                    if let Err(error) = self
                        .index_reconciliation_candidate(folder_id, candidate)
                        .await
                    {
                        drop(receiver);
                        let _ = producer.await;
                        return Err(error);
                    }
                }
                Err(error) => {
                    drop(receiver);
                    let _ = producer.await;
                    return Err(error);
                }
            }
        }
        producer.await.map_err(|_| IndexingError::WorkerStopped)?;

        loop {
            let missing = {
                let connection = self.database.connection();
                connection
                    .query_row(
                        "SELECT canonical_path FROM documents
                         WHERE folder_id = ?1
                           AND NOT EXISTS (
                             SELECT 1 FROM reconciliation_seen
                             WHERE reconciliation_seen.folder_id = documents.folder_id
                               AND reconciliation_seen.canonical_path = documents.canonical_path
                           )
                         ORDER BY canonical_path LIMIT 1",
                        [folder_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?
            };
            let Some(path) = missing else {
                break;
            };
            self.delete_document(folder_id, Path::new(&path))?;
        }
        self.database.connection().execute(
            "DELETE FROM reconciliation_seen WHERE folder_id = ?1",
            [folder_id],
        )?;
        Ok(())
    }

    async fn index_reconciliation_candidate(
        &self,
        folder_id: &str,
        candidate: FileCandidate,
    ) -> Result<(), IndexingError> {
        if self.candidate_matches_stored_identity(folder_id, &candidate)? {
            return Ok(());
        }
        let job_id = random_id();
        self.create_single_candidate_job(&job_id, folder_id, candidate)?;
        let runtime = self.runtime(&job_id).await;
        self.run_job(job_id, runtime).await;
        Ok(())
    }

    async fn spawn_job(&self, job_id: JobId) -> Result<(), IndexingError> {
        let runtime = self.runtime(&job_id).await;
        self.reap_runtime_handle(&runtime).await;
        let mut handle = runtime.handle.lock().await;
        if handle.is_some() {
            return Err(IndexingError::AlreadyRunning(job_id));
        }
        runtime.stop.store(false, Ordering::Release);
        let coordinator = self.clone();
        let worker_job_id = job_id.clone();
        let worker_runtime = Arc::clone(&runtime);
        *handle = Some(tokio::spawn(async move {
            coordinator.run_job(worker_job_id, worker_runtime).await;
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

        if matches!(self.job_state(&job_id), Ok(JobState::Discovering)) {
            if let Err(error) = self.run_discovery(&job_id, &runtime).await {
                if !runtime.stop.load(Ordering::Acquire) {
                    let _ = self.fail_active_job(&job_id, &error);
                }
                self.emit_status(&job_id);
                return;
            }
        }
        if runtime.stop.load(Ordering::Acquire)
            || !matches!(self.job_state(&job_id), Ok(JobState::Parsing))
        {
            return;
        }

        let (sender, mut receiver) = mpsc::channel::<PersistedCandidate>(PIPELINE_CAPACITY);
        let database = Arc::clone(&self.database);
        let producer_job_id = job_id.clone();
        let producer = tokio::task::spawn_blocking(move || {
            send_pending_candidates(&database, &producer_job_id, sender)
        });

        while let Some(candidate) = receiver.recv().await {
            if runtime.stop.load(Ordering::Acquire) {
                break;
            }
            self.limiter.yield_to_foreground().await;
            let gate = runtime.gate.lock().await;
            if runtime.stop.load(Ordering::Acquire)
                || !matches!(self.job_state(&job_id), Ok(JobState::Parsing))
            {
                drop(gate);
                break;
            }
            if let Err(error) = self.process_candidate(&job_id, candidate).await {
                let _ = self.fail_active_job(&job_id, &error);
                drop(gate);
                self.emit_status(&job_id);
                drop(receiver);
                let _ = producer.await;
                return;
            }
            drop(gate);
            self.emit_status(&job_id);
        }
        drop(receiver);
        let _ = producer.await;

        if !runtime.stop.load(Ordering::Acquire) {
            let _ = self.cas_state(&job_id, JobState::Parsing, JobState::Completed);
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
        tokio::task::spawn_blocking(move || {
            let stream = discover(&folder, DiscoveryOptions::default())?;
            for candidate in stream {
                if stop.stop.load(Ordering::Acquire) {
                    return Ok::<(), IndexingError>(());
                }
                persist_discovered_candidate(&database, &discovery_job_id, &candidate)?;
                probe.candidate_persisted(1);
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
            let changed = transaction.execute(
                "UPDATE index_jobs
                 SET state = 'parsing',
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1 AND state = 'discovering'",
                [&discovery_job_id],
            )?;
            transaction.commit()?;
            if changed != 1 {
                return Err(IndexingError::StateChanged);
            }
            Ok(())
        })
        .await
        .map_err(|_| IndexingError::WorkerStopped)?
    }

    async fn process_candidate(
        &self,
        job_id: &str,
        candidate: PersistedCandidate,
    ) -> Result<(), IndexingError> {
        self.set_current_path(job_id, &candidate.canonical_path.to_string_lossy())?;
        if candidate.metadata_only || path_is_metadata_only(&candidate.canonical_path)? {
            return self.complete_metadata_only(job_id, &candidate);
        }
        let trusted_path = match self.validate_immediately_before_parse(job_id, &candidate) {
            Ok(path) => path,
            Err(error) => {
                return self.complete_failure(
                    job_id,
                    &candidate,
                    "PATH_TRUST_FAILED",
                    &error.to_string(),
                );
            }
        };
        self.mark_document_parsing(&candidate)?;

        let parser = Arc::clone(&self.parser);
        let parse_path = trusted_path.clone();
        let max_bytes = self.max_file_size_bytes;
        let parsed =
            tokio::task::spawn_blocking(move || parser.parse(&parse_path, max_bytes)).await;
        match parsed {
            Ok(Ok(document)) => self.complete_success(job_id, &candidate, document),
            Ok(Err(error)) => {
                let (code, message) = parser_failure(&error);
                self.complete_failure(job_id, &candidate, code, &message)
            }
            Err(_) => self.complete_failure(
                job_id,
                &candidate,
                "PARSER_WORKER_STOPPED",
                "parser worker stopped unexpectedly",
            ),
        }
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
    ) -> Result<(), IndexingError> {
        let mut connection = self.database.connection();
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO index_jobs
             (id, folder_id, state, completed_files, total_files, last_path, updated_at)
             VALUES (?1, ?2, 'parsing', 0, 1, NULL,
                     strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            params![job_id, folder_id],
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

    fn mark_document_parsing(&self, candidate: &PersistedCandidate) -> Result<(), IndexingError> {
        let folder_id: String = self.database.connection().query_row(
            "SELECT folder_id FROM index_jobs WHERE id = ?1",
            [&candidate.job_id],
            |row| row.get(0),
        )?;
        upsert_metadata(&self.database, &folder_id, candidate, "parsing", None)
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
        upsert_metadata_transaction(&transaction, &folder_id, candidate, "metadata_only", None)?;
        advance_file_transaction(&transaction, job_id, candidate)?;
        transaction.commit()?;
        Ok(())
    }

    fn complete_success(
        &self,
        job_id: &str,
        candidate: &PersistedCandidate,
        document: ParsedDocument,
    ) -> Result<(), IndexingError> {
        let mut connection = self.database.connection();
        let transaction = connection.transaction()?;
        ensure_job_parsing(&transaction, job_id)?;
        let document_id = document_id_for_path(&transaction, &candidate.canonical_path)?
            .ok_or(IndexingError::MissingDocument)?;
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
        transaction.execute(
            "UPDATE documents
             SET parse_state = 'parsed', parse_error_code = NULL,
                 indexed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1",
            [&document_id],
        )?;
        advance_file_transaction(&transaction, job_id, candidate)?;
        transaction.commit()?;
        Ok(())
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
        upsert_metadata_transaction(&transaction, &folder_id, candidate, "failed", Some(code))?;
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
            self.reap_runtime_handle(&runtime).await;
            if runtime.handle.lock().await.is_none() {
                self.runtimes.lock().await.remove(job_id);
            }
        }
    }

    fn load_status(&self, job_id: &str) -> Result<IndexStatus, IndexingError> {
        let connection = self.database.connection();
        let row: Option<(String, i64, i64, Option<String>)> = connection
            .query_row(
                "SELECT state, total_files, completed_files, last_path
                 FROM index_jobs WHERE id = ?1",
                [job_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let (state, total, completed, current_path) =
            row.ok_or_else(|| IndexingError::JobNotFound(job_id.to_owned()))?;
        let mut statement = connection.prepare(
            "SELECT code, file_name, message FROM index_job_errors
             WHERE job_id = ?1 ORDER BY created_at, id",
        )?;
        let errors = statement
            .query_map([job_id], |row| {
                Ok(IndexFailure {
                    code: row.get(0)?,
                    file_name: row.get(1)?,
                    message: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(IndexStatus {
            job_id: job_id.to_owned(),
            state: JobState::from_sql(&state)?,
            total_files: u64::try_from(total).unwrap_or(0),
            completed_files: u64::try_from(completed).unwrap_or(0),
            current_path,
            errors,
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

fn persist_discovered_candidate(
    database: &Database,
    job_id: &str,
    candidate: &FileCandidate,
) -> Result<(), IndexingError> {
    let mut connection = database.connection();
    let transaction = connection.transaction()?;
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
        transaction.execute(
            "UPDATE index_jobs
             SET total_files = total_files + 1,
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1 AND state = 'discovering'",
            [job_id],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

fn send_pending_candidates(
    database: &Database,
    job_id: &str,
    sender: mpsc::Sender<PersistedCandidate>,
) -> Result<(), IndexingError> {
    let mut last_relative_path = String::new();
    let mut last_canonical_path = String::new();
    loop {
        let batch = {
            let connection = database.connection();
            let mut statement = connection.prepare(
                "SELECT canonical_path, relative_path, size_bytes, modified_at, metadata_only
                 FROM index_job_files
                 WHERE job_id = ?1
                   AND state != 'completed'
                   AND (
                     relative_path > ?2
                     OR (relative_path = ?2 AND canonical_path > ?3)
                   )
                 ORDER BY relative_path, canonical_path
                 LIMIT ?4",
            )?;
            let rows = statement
                .query_map(
                    params![
                        job_id,
                        last_relative_path,
                        last_canonical_path,
                        i64::try_from(PIPELINE_CAPACITY).unwrap_or(16)
                    ],
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
            rows
        };
        let Some(last) = batch.last() else {
            return Ok(());
        };
        last_relative_path.clone_from(&last.relative_path);
        last_canonical_path = last.canonical_path.to_string_lossy().into_owned();
        for candidate in batch {
            if sender.blocking_send(candidate).is_err() {
                return Ok(());
            }
        }
    }
}

fn upsert_metadata(
    database: &Database,
    folder_id: &str,
    candidate: &PersistedCandidate,
    parse_state: &str,
    error_code: Option<&str>,
) -> Result<(), IndexingError> {
    let connection = database.connection();
    upsert_metadata_transaction(&connection, folder_id, candidate, parse_state, error_code)
}

fn upsert_metadata_transaction(
    connection: &rusqlite::Connection,
    folder_id: &str,
    candidate: &PersistedCandidate,
    parse_state: &str,
    error_code: Option<&str>,
) -> Result<(), IndexingError> {
    let canonical_path = candidate.canonical_path.to_string_lossy().into_owned();
    let name = file_name(&candidate.canonical_path);
    let extension = candidate
        .canonical_path
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    connection.execute(
        "INSERT INTO documents
         (id, folder_id, canonical_path, file_name, extension, size_bytes,
          modified_at, parse_state, parse_error_code)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(canonical_path) DO UPDATE SET
           folder_id = excluded.folder_id,
           file_name = excluded.file_name,
           extension = excluded.extension,
           size_bytes = excluded.size_bytes,
           modified_at = excluded.modified_at,
           parse_state = excluded.parse_state,
           parse_error_code = excluded.parse_error_code",
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
        ],
    )?;
    Ok(())
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
    transaction.execute(
        "UPDATE index_job_files SET state = 'completed'
         WHERE job_id = ?1 AND canonical_path = ?2 AND state != 'completed'",
        params![job_id, candidate.canonical_path.to_string_lossy()],
    )?;
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

fn document_id_for_path(
    transaction: &rusqlite::Transaction<'_>,
    path: &Path,
) -> Result<Option<String>, rusqlite::Error> {
    transaction
        .query_row(
            "SELECT id FROM documents WHERE canonical_path = ?1",
            [path.to_string_lossy()],
            |row| row.get(0),
        )
        .optional()
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
            Self::Discovery(_) => "INDEX_DISCOVERY_FAILED",
            Self::Database(_) => "INDEX_DATABASE_ERROR",
            Self::Serialization(_) => "INDEX_SERIALIZATION_ERROR",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{trusted_event_identity, EventPathProvider};

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
}
