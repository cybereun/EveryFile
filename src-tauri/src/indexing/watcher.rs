use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use notify::event::{ModifyKind, RenameMode};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

use super::{IndexCoordinator, IndexingError};

const DEBOUNCE: Duration = Duration::from_millis(500);
const WATCH_CHANNEL_CAPACITY: usize = 64;

#[derive(Debug, Clone)]
pub enum WatchChange {
    Write(PathBuf),
    Rename {
        from: PathBuf,
        to: PathBuf,
    },
    RenameFrom {
        path: PathBuf,
        tracker: Option<usize>,
    },
    RenameTo {
        path: PathBuf,
        tracker: Option<usize>,
    },
    Delete(PathBuf),
}

#[derive(Clone)]
pub struct IndexWatcher {
    inner: Arc<WatcherInner>,
}

struct WatcherInner {
    sender: mpsc::Sender<WatcherMessage>,
    overflowed: Arc<AtomicBool>,
    _watcher: parking_lot::Mutex<Option<RecommendedWatcher>>,
    tasks: parking_lot::Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl Drop for WatcherInner {
    fn drop(&mut self) {
        for task in self.tasks.get_mut().drain(..) {
            task.abort();
        }
    }
}

enum WatcherMessage {
    Change(WatchChange),
    Flush(oneshot::Sender<Result<(), String>>),
}

impl IndexWatcher {
    pub async fn start(
        coordinator: Arc<IndexCoordinator>,
        folder_id: String,
    ) -> Result<Self, WatcherError> {
        let root = coordinator.registered_root(&folder_id)?;
        let (sender, receiver) = mpsc::channel(WATCH_CHANNEL_CAPACITY);
        let callback_sender = sender.clone();
        let overflowed = Arc::new(AtomicBool::new(false));
        let callback_overflowed = Arc::clone(&overflowed);
        let callback_coordinator = Arc::clone(&coordinator);
        let callback_folder_id = folder_id.clone();
        let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
            let Ok(event) = result else {
                callback_overflowed.store(true, Ordering::Release);
                let _ = callback_coordinator.record_folder_diagnostic(
                    &callback_folder_id,
                    "WATCHER_EVENT_ERROR",
                    "the operating-system watcher reported an event error",
                );
                return;
            };
            for change in changes_from_event(event) {
                if callback_sender
                    .try_send(WatcherMessage::Change(change))
                    .is_err()
                {
                    callback_overflowed.store(true, Ordering::Release);
                }
            }
        })?;
        watcher.watch(&root, RecursiveMode::Recursive)?;
        let worker = tokio::spawn(watcher_loop(
            Arc::clone(&coordinator),
            folder_id.clone(),
            receiver,
            Arc::clone(&overflowed),
        ));
        let reconciliation = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(15 * 60));
            interval.tick().await;
            loop {
                interval.tick().await;
                if let Err(error) = coordinator.reconcile(&folder_id).await {
                    let _ = coordinator.record_folder_diagnostic(
                        &folder_id,
                        "WATCHER_RECONCILE_FAILED",
                        &error.to_string(),
                    );
                }
            }
        });

        Ok(Self {
            inner: Arc::new(WatcherInner {
                sender,
                overflowed,
                _watcher: parking_lot::Mutex::new(Some(watcher)),
                tasks: parking_lot::Mutex::new(vec![worker, reconciliation]),
            }),
        })
    }

    pub async fn ingest(&self, change: WatchChange) -> Result<(), WatcherError> {
        self.inner
            .sender
            .send(WatcherMessage::Change(change))
            .await
            .map_err(|_| WatcherError::Stopped)
    }

    pub fn ingest_nowait(&self, change: WatchChange) -> Result<(), WatcherError> {
        self.inner
            .sender
            .try_send(WatcherMessage::Change(change))
            .map_err(|error| {
                self.inner.overflowed.store(true, Ordering::Release);
                match error {
                    mpsc::error::TrySendError::Closed(_) => WatcherError::Stopped,
                    mpsc::error::TrySendError::Full(_) => WatcherError::Overflow,
                }
            })
    }

    pub async fn flush(&self) -> Result<(), WatcherError> {
        let (sender, receiver) = oneshot::channel();
        self.inner
            .sender
            .send(WatcherMessage::Flush(sender))
            .await
            .map_err(|_| WatcherError::Stopped)?;
        receiver
            .await
            .map_err(|_| WatcherError::Stopped)?
            .map_err(WatcherError::Indexing)
    }
}

async fn watcher_loop(
    coordinator: Arc<IndexCoordinator>,
    folder_id: String,
    mut receiver: mpsc::Receiver<WatcherMessage>,
    overflowed: Arc<AtomicBool>,
) {
    let mut pending = ChangeAccumulator::default();
    let mut flush_waiters = Vec::new();
    while let Some(message) = receiver.recv().await {
        match message {
            WatcherMessage::Change(change) => {
                pending.push(change);
            }
            WatcherMessage::Flush(waiter)
                if pending.is_empty() && !overflowed.load(Ordering::Acquire) =>
            {
                let _ = waiter.send(Ok(()));
                continue;
            }
            WatcherMessage::Flush(waiter) => flush_waiters.push(waiter),
        }

        let mut reconcile_overflow = overflowed.swap(false, Ordering::AcqRel);
        let timer = tokio::time::sleep(DEBOUNCE);
        tokio::pin!(timer);
        loop {
            if overflowed.swap(false, Ordering::AcqRel) {
                reconcile_overflow = true;
                break;
            }
            tokio::select! {
                message = receiver.recv() => {
                    match message {
                        Some(WatcherMessage::Change(change)) => {
                            pending.push(change);
                            if overflowed.swap(false, Ordering::AcqRel) {
                                reconcile_overflow = true;
                                break;
                            }
                            timer.as_mut().reset(tokio::time::Instant::now() + DEBOUNCE);
                        }
                        Some(WatcherMessage::Flush(waiter)) => flush_waiters.push(waiter),
                        None => return,
                    }
                }
                _ = &mut timer => break,
            }
        }

        let mut result = Ok(());
        if reconcile_overflow {
            pending.clear();
            let _ = coordinator.record_folder_diagnostic(
                &folder_id,
                "WATCHER_OVERFLOW",
                "watcher event capacity was exceeded; a full reconciliation was started",
            );
            if let Err(error) = coordinator.reconcile(&folder_id).await {
                result = Err(error.to_string());
                let _ = coordinator.record_folder_diagnostic(
                    &folder_id,
                    "WATCHER_RECONCILE_FAILED",
                    &error.to_string(),
                );
            }
        }
        for change in pending.drain() {
            let applied = match change {
                WatchChange::Write(path) => {
                    coordinator.reindex_discovered_path(&folder_id, &path).await
                }
                WatchChange::Rename { from, to } => {
                    coordinator.reconcile_rename(&folder_id, &from, &to).await
                }
                WatchChange::Delete(path) => coordinator.delete_document(&folder_id, &path),
                WatchChange::RenameFrom { .. } | WatchChange::RenameTo { .. } => {
                    unreachable!("rename halves are resolved before draining")
                }
            };
            if let Err(error) = applied {
                result = Err(error.to_string());
                let _ = coordinator.record_folder_diagnostic(
                    &folder_id,
                    "WATCHER_CHANGE_FAILED",
                    &error.to_string(),
                );
            }
        }
        for waiter in flush_waiters.drain(..) {
            let _ = waiter.send(result.clone());
        }
    }
}

#[derive(Default)]
struct ChangeAccumulator {
    next_sequence: u64,
    operations: Vec<(u64, Option<WatchChange>)>,
    tracked_from: HashMap<usize, (u64, PathBuf)>,
    untracked_from: VecDeque<(u64, PathBuf)>,
}

impl ChangeAccumulator {
    fn is_empty(&self) -> bool {
        self.operations.is_empty() && self.tracked_from.is_empty() && self.untracked_from.is_empty()
    }

    fn clear(&mut self) {
        self.operations.clear();
        self.tracked_from.clear();
        self.untracked_from.clear();
    }

    fn push(&mut self, change: WatchChange) {
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        match change {
            WatchChange::RenameFrom { path, tracker } => {
                if let Some(tracker) = tracker {
                    if let Some((old_sequence, old_path)) =
                        self.tracked_from.insert(tracker, (sequence, path))
                    {
                        self.push_at(old_sequence, WatchChange::Delete(old_path));
                    }
                } else {
                    self.untracked_from.push_back((sequence, path));
                }
            }
            WatchChange::RenameTo { path, tracker } => {
                let from = tracker
                    .and_then(|tracker| self.tracked_from.remove(&tracker))
                    .or_else(|| {
                        if tracker.is_none() {
                            self.untracked_from.pop_front()
                        } else {
                            None
                        }
                    });
                if let Some((_, from)) = from {
                    self.push_at(sequence, WatchChange::Rename { from, to: path });
                } else {
                    self.push_at(sequence, WatchChange::Write(path));
                }
            }
            change => self.push_at(sequence, change),
        }
    }

    fn push_at(&mut self, sequence: u64, change: WatchChange) {
        if let Some(key) = coalescible_path_key(&change) {
            for (_, pending) in &mut self.operations {
                if pending
                    .as_ref()
                    .and_then(coalescible_path_key)
                    .is_some_and(|pending_key| pending_key == key)
                {
                    *pending = None;
                }
            }
        }
        self.operations.push((sequence, Some(change)));
    }

    fn drain(&mut self) -> Vec<WatchChange> {
        let tracked = std::mem::take(&mut self.tracked_from);
        for (_, (sequence, path)) in tracked {
            self.operations
                .push((sequence, Some(WatchChange::Delete(path))));
        }
        for (sequence, path) in self.untracked_from.drain(..) {
            self.operations
                .push((sequence, Some(WatchChange::Delete(path))));
        }
        self.operations.sort_by_key(|(sequence, _)| *sequence);
        std::mem::take(&mut self.operations)
            .into_iter()
            .filter_map(|(_, change)| change)
            .collect()
    }
}

fn changes_from_event(event: Event) -> Vec<WatchChange> {
    match event.kind {
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if event.paths.len() >= 2 => {
            vec![WatchChange::Rename {
                from: event.paths[0].clone(),
                to: event.paths[1].clone(),
            }]
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => event
            .paths
            .into_iter()
            .map(|path| WatchChange::RenameFrom {
                path,
                tracker: event.attrs.tracker(),
            })
            .collect(),
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => event
            .paths
            .into_iter()
            .map(|path| WatchChange::RenameTo {
                path,
                tracker: event.attrs.tracker(),
            })
            .collect(),
        EventKind::Remove(_) => event.paths.into_iter().map(WatchChange::Delete).collect(),
        EventKind::Create(_) | EventKind::Modify(_) => {
            event.paths.into_iter().map(WatchChange::Write).collect()
        }
        _ => Vec::new(),
    }
}

fn coalescible_path_key(change: &WatchChange) -> Option<String> {
    match change {
        WatchChange::Write(path) | WatchChange::Delete(path) => Some(normalized_path_key(path)),
        WatchChange::Rename { .. }
        | WatchChange::RenameFrom { .. }
        | WatchChange::RenameTo { .. } => None,
    }
}

pub(crate) fn normalized_path_key(path: &std::path::Path) -> String {
    path.to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('/', r"\")
        .to_lowercase()
}

#[derive(Debug, Error)]
pub enum WatcherError {
    #[error("file watcher setup failed")]
    Notify(#[from] notify::Error),
    #[error("file watcher stopped")]
    Stopped,
    #[error("file watcher event queue overflowed; reconciliation scheduled")]
    Overflow,
    #[error("file watcher reconciliation failed: {0}")]
    Indexing(String),
    #[error(transparent)]
    Coordinator(#[from] IndexingError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{EventAttributes, ModifyKind};
    use std::path::Path;

    #[test]
    fn from_and_to_notifications_are_correlated_by_tracker() {
        let mut from_attributes = EventAttributes::new();
        from_attributes.set_tracker(41);
        let from = Event {
            kind: EventKind::Modify(ModifyKind::Name(RenameMode::From)),
            paths: vec![PathBuf::from(r"C:\docs\old.txt")],
            attrs: from_attributes,
        };
        let mut to_attributes = EventAttributes::new();
        to_attributes.set_tracker(41);
        let to = Event {
            kind: EventKind::Modify(ModifyKind::Name(RenameMode::To)),
            paths: vec![PathBuf::from(r"C:\docs\new.txt")],
            attrs: to_attributes,
        };

        let mut accumulator = ChangeAccumulator::default();
        for change in changes_from_event(from)
            .into_iter()
            .chain(changes_from_event(to))
        {
            accumulator.push(change);
        }

        assert!(matches!(
            accumulator.drain().as_slice(),
            [WatchChange::Rename { from, to }]
                if from == Path::new(r"C:\docs\old.txt")
                    && to == Path::new(r"C:\docs\new.txt")
        ));
    }

    #[test]
    fn coalescing_is_deterministic_and_preserves_final_path_semantics() {
        let mut accumulator = ChangeAccumulator::default();
        accumulator.push(WatchChange::Write(PathBuf::from(r"C:\docs\b.txt")));
        accumulator.push(WatchChange::Delete(PathBuf::from(r"C:\docs\a.txt")));
        accumulator.push(WatchChange::Delete(PathBuf::from(r"C:\docs\b.txt")));
        accumulator.push(WatchChange::Write(PathBuf::from(r"C:\docs\b.txt")));

        let changes = accumulator.drain();
        assert!(matches!(
            changes.as_slice(),
            [WatchChange::Delete(a), WatchChange::Write(b)]
                if a == Path::new(r"C:\docs\a.txt") && b == Path::new(r"C:\docs\b.txt")
        ));
    }

    #[test]
    fn deleted_placeholder_keys_are_normalized_without_opening_the_path() {
        let missing = Path::new(r"\\?\C:\DOCS\missing.txt");
        assert_eq!(normalized_path_key(missing), r"c:\docs\missing.txt");
    }
}
