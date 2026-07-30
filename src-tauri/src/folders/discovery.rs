use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use ignore::{DirEntry, WalkBuilder};
use serde::Serialize;
use thiserror::Error;

use crate::domain::models::FolderRecord;

const DEFAULT_EXCLUDED_DIRECTORIES: &[&str] = &[".git", "node_modules", "$RECYCLE.BIN"];

#[derive(Debug, Clone)]
pub struct DiscoveryOptions {
    excluded_directories: HashSet<String>,
    excluded_path_patterns: Vec<String>,
}

impl Default for DiscoveryOptions {
    fn default() -> Self {
        Self {
            excluded_directories: DEFAULT_EXCLUDED_DIRECTORIES
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
            excluded_path_patterns: Vec::new(),
        }
    }
}

impl DiscoveryOptions {
    pub fn with_excluded_directory(mut self, name: impl Into<String>) -> Self {
        self.excluded_directories.insert(name.into());
        self
    }

    pub fn with_excluded_path_patterns(mut self, patterns: Vec<String>) -> Self {
        self.excluded_path_patterns = patterns;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileCandidate {
    pub canonical_path: PathBuf,
    pub relative_path: String,
    pub size_bytes: u64,
    pub modified_at: Option<SystemTime>,
    pub metadata_only: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryWarning {
    pub code: String,
    pub path: Option<String>,
    pub message: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct DiscoveryReport {
    pub files: Vec<FileCandidate>,
    pub warnings: Vec<DiscoveryWarning>,
}

const DISCOVERY_CHANNEL_CAPACITY: usize = 32;

enum DiscoveryMessage {
    Candidate(FileCandidate),
    Warning(DiscoveryWarning),
}

struct DiscoverySender {
    sender: SyncSender<DiscoveryMessage>,
    cancelled: Arc<AtomicBool>,
}

impl DiscoverySender {
    fn send_candidate(&self, candidate: FileCandidate) -> bool {
        self.send(DiscoveryMessage::Candidate(candidate))
    }

    fn send_warning(&self, warning: DiscoveryWarning) -> bool {
        self.send(DiscoveryMessage::Warning(warning))
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn send(&self, mut message: DiscoveryMessage) -> bool {
        loop {
            if self.is_cancelled() {
                return false;
            }
            match self.sender.try_send(message) {
                Ok(()) => return true,
                Err(TrySendError::Full(returned)) => {
                    message = returned;
                    thread::sleep(Duration::from_millis(1));
                }
                Err(TrySendError::Disconnected(_)) => return false,
            }
        }
    }
}

pub struct DiscoveryStream {
    receiver: Receiver<DiscoveryMessage>,
    warnings: Vec<DiscoveryWarning>,
    cancelled: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl DiscoveryStream {
    pub fn warnings(&self) -> &[DiscoveryWarning] {
        &self.warnings
    }

    pub(crate) fn next_with_timeout(&mut self, timeout: Duration) -> DiscoveryPoll {
        loop {
            match self.receiver.recv_timeout(timeout) {
                Ok(DiscoveryMessage::Candidate(candidate)) => {
                    return DiscoveryPoll::Candidate(candidate);
                }
                Ok(DiscoveryMessage::Warning(warning)) => self.warnings.push(warning),
                Err(RecvTimeoutError::Timeout) => return DiscoveryPoll::Pending,
                Err(RecvTimeoutError::Disconnected) => {
                    self.join_finished_worker();
                    return DiscoveryPoll::Finished;
                }
            }
        }
    }
}

pub(crate) enum DiscoveryPoll {
    Candidate(FileCandidate),
    Pending,
    Finished,
}

impl Iterator for DiscoveryStream {
    type Item = FileCandidate;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.receiver.recv() {
                Ok(DiscoveryMessage::Candidate(candidate)) => return Some(candidate),
                Ok(DiscoveryMessage::Warning(warning)) => self.warnings.push(warning),
                Err(_) => {
                    self.join_finished_worker();
                    return None;
                }
            }
        }
    }
}

impl DiscoveryStream {
    pub(crate) fn cancel_and_join(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        self.join_finished_worker();
    }

    fn join_finished_worker(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for DiscoveryStream {
    fn drop(&mut self) {
        self.cancel_and_join();
    }
}

pub fn discover(
    folder: &FolderRecord,
    options: DiscoveryOptions,
) -> Result<DiscoveryStream, DiscoveryError> {
    discover_path(Path::new(&folder.canonical_path), options)
}

pub fn discover_all(
    selected_root: &Path,
    options: DiscoveryOptions,
) -> Result<DiscoveryReport, DiscoveryError> {
    let mut stream = discover_path(selected_root, options)?;
    let files = stream.by_ref().collect();
    let warnings = stream.warnings().to_vec();
    Ok(DiscoveryReport { files, warnings })
}

fn discover_path(
    selected_root: &Path,
    options: DiscoveryOptions,
) -> Result<DiscoveryStream, DiscoveryError> {
    let canonical_root =
        selected_root
            .canonicalize()
            .map_err(|source| DiscoveryError::RootUnavailable {
                path: selected_root.to_path_buf(),
                source,
            })?;
    if !canonical_root.is_dir() {
        return Err(DiscoveryError::RootIsNotDirectory(canonical_root));
    }

    Ok(spawn_discovery_worker(move |sender| {
        walk_root(canonical_root, options, sender);
    }))
}

fn spawn_discovery_worker<F>(producer: F) -> DiscoveryStream
where
    F: FnOnce(DiscoverySender) + Send + 'static,
{
    let (sender, receiver) = mpsc::sync_channel(DISCOVERY_CHANNEL_CAPACITY);
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = Arc::clone(&cancelled);
    let worker = thread::spawn(move || {
        producer(DiscoverySender {
            sender,
            cancelled: worker_cancelled,
        });
    });

    DiscoveryStream {
        receiver,
        warnings: Vec::new(),
        cancelled,
        worker: Some(worker),
    }
}

fn walk_root(canonical_root: PathBuf, options: DiscoveryOptions, sender: DiscoverySender) {
    let excluded_directories = Arc::new(options.excluded_directories);
    let excluded_path_patterns = Arc::new(options.excluded_path_patterns);
    let filter_exclusions = Arc::clone(&excluded_directories);
    let filter_patterns = Arc::clone(&excluded_path_patterns);
    let filter_root = canonical_root.clone();
    let mut builder = WalkBuilder::new(&canonical_root);
    builder
        .hidden(false)
        .follow_links(false)
        .ignore(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .parents(false)
        .sort_by_file_path(|left, right| left.cmp(right))
        .filter_entry(move |entry| {
            should_descend(entry, &filter_root, &filter_exclusions, &filter_patterns)
        });

    let path_open = OsPathOpenProvider;
    for result in builder.build() {
        if sender.is_cancelled() {
            return;
        }
        let entry = match result {
            Ok(entry) => entry,
            Err(error) => {
                if !sender.send_warning(walk_warning(&error)) {
                    return;
                }
                continue;
            }
        };
        if entry.depth() == 0 {
            continue;
        }

        let snapshot = match snapshot_entry(&entry) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                if !sender.send_warning(walk_warning(&error)) {
                    return;
                }
                continue;
            }
        };
        if path_matches_exclusion(&canonical_root, &snapshot.path, &excluded_path_patterns) {
            continue;
        }
        if !matches!(
            classify_snapshot(&snapshot, &excluded_directories),
            EntryClassification::Candidate { .. }
        ) {
            continue;
        }

        match candidate_from_snapshot(&canonical_root, snapshot, &path_open) {
            Ok(Some(candidate)) => {
                if !sender.send_candidate(candidate) {
                    return;
                }
            }
            Ok(None) => {}
            Err(warning) => {
                if !sender.send_warning(warning) {
                    return;
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiscoveryEntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

#[derive(Debug)]
struct DiscoveryEntrySnapshot {
    path: PathBuf,
    canonical_parent: PathBuf,
    file_name: std::ffi::OsString,
    kind: DiscoveryEntryKind,
    attributes: u32,
    size_bytes: u64,
    modified_at: Option<SystemTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryClassification {
    PruneDirectory,
    Descend,
    Skip,
    Candidate { metadata_only: bool },
}

trait PathOpenProvider {
    fn canonicalize_hydrated(&self, path: &Path) -> io::Result<PathBuf>;
}

struct OsPathOpenProvider;

impl PathOpenProvider for OsPathOpenProvider {
    fn canonicalize_hydrated(&self, path: &Path) -> io::Result<PathBuf> {
        path.canonicalize()
    }
}

fn snapshot_entry(entry: &DirEntry) -> Result<DiscoveryEntrySnapshot, ignore::Error> {
    let metadata = entry.metadata()?;
    let kind = if entry.path_is_symlink() {
        DiscoveryEntryKind::Symlink
    } else {
        match entry.file_type() {
            Some(file_type) if file_type.is_file() => DiscoveryEntryKind::File,
            Some(file_type) if file_type.is_dir() => DiscoveryEntryKind::Directory,
            Some(_) => DiscoveryEntryKind::Other,
            None => DiscoveryEntryKind::Other,
        }
    };
    let canonical_parent = entry
        .path()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();

    Ok(DiscoveryEntrySnapshot {
        path: entry.path().to_path_buf(),
        canonical_parent,
        file_name: entry.file_name().to_os_string(),
        kind,
        attributes: enumerated_attributes(&metadata),
        size_bytes: metadata.len(),
        modified_at: metadata.modified().ok(),
    })
}

fn classify_snapshot(
    snapshot: &DiscoveryEntrySnapshot,
    exclusions: &HashSet<String>,
) -> EntryClassification {
    match snapshot.kind {
        DiscoveryEntryKind::Symlink => EntryClassification::PruneDirectory,
        DiscoveryEntryKind::Directory
            if is_directory_reparse_attributes(snapshot.attributes)
                || snapshot.file_name.to_str().is_some_and(|name| {
                    exclusions
                        .iter()
                        .any(|excluded| name.eq_ignore_ascii_case(excluded))
                }) =>
        {
            EntryClassification::PruneDirectory
        }
        DiscoveryEntryKind::Directory => EntryClassification::Descend,
        DiscoveryEntryKind::File => EntryClassification::Candidate {
            metadata_only: metadata_only_attributes(snapshot.attributes),
        },
        DiscoveryEntryKind::Other => EntryClassification::Skip,
    }
}

fn should_descend(
    entry: &DirEntry,
    root: &Path,
    exclusions: &HashSet<String>,
    patterns: &[String],
) -> bool {
    if entry.depth() == 0 {
        return true;
    }

    snapshot_entry(entry).map_or(true, |snapshot| {
        if path_matches_exclusion(root, &snapshot.path, patterns) {
            return false;
        }
        !matches!(
            classify_snapshot(&snapshot, exclusions),
            EntryClassification::PruneDirectory
        )
    })
}

fn path_matches_exclusion(root: &Path, path: &Path, patterns: &[String]) -> bool {
    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase();
    patterns.iter().any(|pattern| {
        let normalized = pattern.trim().replace('\\', "/").to_lowercase();
        wildcard_path_match(&normalized, &relative)
            || path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| wildcard_path_match(&normalized, &name.to_lowercase()))
    })
}

fn wildcard_path_match(pattern: &str, value: &str) -> bool {
    let pattern = pattern.as_bytes();
    let value = value.as_bytes();
    let (mut pattern_index, mut value_index) = (0, 0);
    let (mut star, mut retry_value) = (None, 0);
    while value_index < value.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?' || pattern[pattern_index] == value[value_index])
        {
            pattern_index += 1;
            value_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
                pattern_index += 1;
            }
            star = Some(pattern_index);
            retry_value = value_index;
        } else if let Some(after_star) = star {
            retry_value += 1;
            value_index = retry_value;
            pattern_index = after_star;
        } else {
            return false;
        }
    }
    pattern[pattern_index..].iter().all(|byte| *byte == b'*')
}

fn candidate_from_snapshot(
    canonical_root: &Path,
    snapshot: DiscoveryEntrySnapshot,
    path_open: &dyn PathOpenProvider,
) -> Result<Option<FileCandidate>, DiscoveryWarning> {
    if snapshot.kind != DiscoveryEntryKind::File {
        return Ok(None);
    }

    let metadata_only = metadata_only_attributes(snapshot.attributes);
    let canonical_path = if metadata_only {
        snapshot.canonical_parent.join(&snapshot.file_name)
    } else {
        path_open
            .canonicalize_hydrated(&snapshot.path)
            .map_err(|error| io_warning("ENTRY_CANONICALIZE_FAILED", &snapshot.path, error))?
    };
    if !canonical_path.starts_with(canonical_root) {
        return Err(DiscoveryWarning {
            code: "ENTRY_OUTSIDE_ROOT".into(),
            path: Some(snapshot.path.to_string_lossy().into_owned()),
            message: "candidate resolved outside the registered root".into(),
        });
    }
    let relative_path = canonical_path
        .strip_prefix(canonical_root)
        .map_err(|_| DiscoveryWarning {
            code: "ENTRY_OUTSIDE_ROOT".into(),
            path: Some(snapshot.path.to_string_lossy().into_owned()),
            message: "candidate resolved outside the registered root".into(),
        })?
        .to_string_lossy()
        .replace('\\', "/");

    Ok(Some(FileCandidate {
        canonical_path,
        relative_path,
        size_bytes: snapshot.size_bytes,
        modified_at: snapshot.modified_at,
        metadata_only,
    }))
}

#[cfg(windows)]
fn enumerated_attributes(metadata: &fs::Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;

    metadata.file_attributes()
}

#[cfg(not(windows))]
fn enumerated_attributes(_metadata: &fs::Metadata) -> u32 {
    0
}

#[cfg(windows)]
fn is_directory_reparse_attributes(attributes: u32) -> bool {
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

    attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_directory_reparse_attributes(_attributes: u32) -> bool {
    false
}

#[cfg(windows)]
fn metadata_only_attributes(attributes: u32) -> bool {
    is_metadata_only_file_attributes(attributes)
}

#[cfg(not(windows))]
fn metadata_only_attributes(_attributes: u32) -> bool {
    false
}

#[cfg(windows)]
pub fn is_metadata_only_file_attributes(attributes: u32) -> bool {
    const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
    const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
    const PLACEHOLDER_ATTRIBUTES: u32 = FILE_ATTRIBUTE_OFFLINE
        | FILE_ATTRIBUTE_RECALL_ON_OPEN
        | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS;

    attributes & PLACEHOLDER_ATTRIBUTES != 0
}

fn walk_warning(error: &ignore::Error) -> DiscoveryWarning {
    DiscoveryWarning {
        code: "ENTRY_INACCESSIBLE".into(),
        path: walk_error_path(error).map(|path| path.to_string_lossy().into_owned()),
        message: error.to_string(),
    }
}

fn walk_error_path(error: &ignore::Error) -> Option<&Path> {
    match error {
        ignore::Error::WithPath { path, .. } => Some(path),
        ignore::Error::WithLineNumber { err, .. } | ignore::Error::WithDepth { err, .. } => {
            walk_error_path(err)
        }
        ignore::Error::Loop { child, .. } => Some(child),
        ignore::Error::Partial(errors) => errors.iter().find_map(walk_error_path),
        _ => None,
    }
}

fn io_warning(code: &str, path: &Path, error: std::io::Error) -> DiscoveryWarning {
    DiscoveryWarning {
        code: code.into(),
        path: Some(path.to_string_lossy().into_owned()),
        message: error.to_string(),
    }
}

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("registered root is unavailable: {path}")]
    RootUnavailable {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("registered root is not a directory: {0}")]
    RootIsNotDirectory(PathBuf),
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{mpsc, Arc};
    use std::thread;
    use std::time::{Duration, SystemTime};

    use super::{
        candidate_from_snapshot, classify_snapshot, discover_all, spawn_discovery_worker,
        DiscoveryEntryKind, DiscoveryEntrySnapshot, DiscoveryOptions, EntryClassification,
        FileCandidate, PathOpenProvider, DISCOVERY_CHANNEL_CAPACITY,
    };

    struct PanicPathOpenProvider;

    impl PathOpenProvider for PanicPathOpenProvider {
        fn canonicalize_hydrated(&self, _path: &Path) -> io::Result<PathBuf> {
            panic!("metadata-only and pruned entries must not open or canonicalize their path")
        }
    }

    fn candidate(relative_path: &str) -> FileCandidate {
        FileCandidate {
            canonical_path: PathBuf::from("C:\\fixture").join(relative_path),
            relative_path: relative_path.into(),
            size_bytes: 5,
            modified_at: Some(SystemTime::UNIX_EPOCH),
            metadata_only: false,
        }
    }

    #[cfg(windows)]
    #[test]
    fn metadata_only_snapshot_never_invokes_path_open_provider() {
        const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;

        let root = PathBuf::from("C:\\fixture");
        let snapshot = DiscoveryEntrySnapshot {
            path: root.join("cloud").join("report.pdf"),
            canonical_parent: root.join("cloud"),
            file_name: "report.pdf".into(),
            kind: DiscoveryEntryKind::File,
            attributes: FILE_ATTRIBUTE_OFFLINE,
            size_bytes: 42,
            modified_at: Some(SystemTime::UNIX_EPOCH),
        };

        let result = candidate_from_snapshot(&root, snapshot, &PanicPathOpenProvider).unwrap();

        assert_eq!(
            result.unwrap(),
            FileCandidate {
                canonical_path: root.join("cloud").join("report.pdf"),
                relative_path: "cloud/report.pdf".into(),
                size_bytes: 42,
                modified_at: Some(SystemTime::UNIX_EPOCH),
                metadata_only: true,
            }
        );
    }

    #[cfg(windows)]
    #[test]
    fn directory_reparse_snapshot_is_pruned_before_path_open() {
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

        let root = PathBuf::from("C:\\fixture");
        let snapshot = DiscoveryEntrySnapshot {
            path: root.join("junction"),
            canonical_parent: root.clone(),
            file_name: "junction".into(),
            kind: DiscoveryEntryKind::Directory,
            attributes: FILE_ATTRIBUTE_REPARSE_POINT,
            size_bytes: 0,
            modified_at: None,
        };

        assert_eq!(
            classify_snapshot(&snapshot, &HashSet::new()),
            EntryClassification::PruneDirectory
        );
    }

    #[test]
    fn first_candidate_is_observable_while_traversal_is_blocked() {
        let (reached_gate_tx, reached_gate_rx) = mpsc::sync_channel(0);
        let (release_gate_tx, release_gate_rx) = mpsc::sync_channel(0);
        let mut stream = spawn_discovery_worker(move |sender| {
            assert!(sender.send_candidate(candidate("first.txt")));
            reached_gate_tx.send(()).unwrap();
            release_gate_rx.recv().unwrap();
            let _ = sender.send_candidate(candidate("second.txt"));
        });

        reached_gate_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        assert_eq!(stream.next().unwrap().relative_path, "first.txt");

        release_gate_tx.send(()).unwrap();
        assert_eq!(stream.next().unwrap().relative_path, "second.txt");
        assert!(stream.next().is_none());
    }

    #[test]
    fn dropping_stream_cancels_a_bounded_producer() {
        let produced = Arc::new(AtomicUsize::new(0));
        let worker_produced = Arc::clone(&produced);
        let (stopped_tx, stopped_rx) = mpsc::sync_channel(1);
        let stream = spawn_discovery_worker(move |sender| {
            while sender.send_candidate(candidate("queued.txt")) {
                worker_produced.fetch_add(1, Ordering::SeqCst);
            }
            stopped_tx.send(()).unwrap();
        });

        drop(stream);

        stopped_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(produced.load(Ordering::SeqCst) <= DISCOVERY_CHANNEL_CAPACITY);
    }

    #[test]
    fn dropping_stream_cancels_and_joins_a_gated_producer() {
        let active_workers = Arc::new(AtomicUsize::new(0));
        let worker_count = Arc::clone(&active_workers);
        let (started_tx, started_rx) = mpsc::sync_channel(0);
        let (cancelled_tx, cancelled_rx) = mpsc::sync_channel(0);
        let (release_tx, release_rx) = mpsc::sync_channel(0);
        let stream = spawn_discovery_worker(move |sender| {
            worker_count.fetch_add(1, Ordering::SeqCst);
            started_tx.send(()).unwrap();
            while !sender.is_cancelled() {
                thread::yield_now();
            }
            cancelled_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            worker_count.fetch_sub(1, Ordering::SeqCst);
        });
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();

        let (drop_finished_tx, drop_finished_rx) = mpsc::sync_channel(0);
        let dropper = thread::spawn(move || {
            drop(stream);
            drop_finished_tx.send(()).unwrap();
        });
        cancelled_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("stream drop did not cancel its producer");

        assert!(
            drop_finished_rx
                .recv_timeout(Duration::from_millis(100))
                .is_err(),
            "stream drop returned while its discovery worker was still active"
        );
        assert_eq!(active_workers.load(Ordering::SeqCst), 1);

        release_tx.send(()).unwrap();
        drop_finished_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("stream drop did not join its released worker");
        dropper.join().unwrap();
        assert_eq!(active_workers.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn configured_path_patterns_exclude_matching_files_and_subtrees() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("cache").join("nested")).unwrap();
        std::fs::write(temp.path().join("keep.txt"), b"keep").unwrap();
        std::fs::write(temp.path().join("secret.tmp"), b"secret").unwrap();
        std::fs::write(
            temp.path().join("cache").join("nested").join("hidden.txt"),
            b"hidden",
        )
        .unwrap();

        let report = discover_all(
            temp.path(),
            DiscoveryOptions::default()
                .with_excluded_path_patterns(vec!["*.tmp".into(), "cache/**".into()]),
        )
        .unwrap();

        assert_eq!(
            report
                .files
                .iter()
                .map(|candidate| candidate.relative_path.as_str())
                .collect::<Vec<_>>(),
            ["keep.txt"]
        );
    }
}
