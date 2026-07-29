use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
    Rename { from: PathBuf, to: PathBuf },
    Delete(PathBuf),
}

#[derive(Clone)]
pub struct IndexWatcher {
    inner: Arc<WatcherInner>,
}

struct WatcherInner {
    sender: mpsc::Sender<WatcherMessage>,
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
        let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
            let Ok(event) = result else {
                return;
            };
            for change in changes_from_event(event) {
                let _ = callback_sender.try_send(WatcherMessage::Change(change));
            }
        })?;
        watcher.watch(&root, RecursiveMode::Recursive)?;
        let worker = tokio::spawn(watcher_loop(
            Arc::clone(&coordinator),
            folder_id.clone(),
            receiver,
        ));
        let reconciliation = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(15 * 60));
            interval.tick().await;
            loop {
                interval.tick().await;
                let _ = coordinator.reconcile(&folder_id).await;
            }
        });

        Ok(Self {
            inner: Arc::new(WatcherInner {
                sender,
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
) {
    let mut pending = HashMap::<String, WatchChange>::new();
    let mut recently_applied = HashMap::<String, Instant>::new();
    let mut flush_waiters = Vec::new();
    while let Some(message) = receiver.recv().await {
        match message {
            WatcherMessage::Change(change) => {
                queue_change(&mut pending, &recently_applied, change);
            }
            WatcherMessage::Flush(waiter) if pending.is_empty() => {
                let _ = waiter.send(Ok(()));
                continue;
            }
            WatcherMessage::Flush(waiter) => flush_waiters.push(waiter),
        }

        let timer = tokio::time::sleep(DEBOUNCE);
        tokio::pin!(timer);
        loop {
            tokio::select! {
                message = receiver.recv() => {
                    match message {
                        Some(WatcherMessage::Change(change)) => {
                            queue_change(&mut pending, &recently_applied, change);
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
        for (key, change) in pending.drain() {
            let applied = match change {
                WatchChange::Write(path) => {
                    coordinator.reindex_discovered_path(&folder_id, &path).await
                }
                WatchChange::Rename { from, to } => {
                    coordinator.reconcile_rename(&folder_id, &from, &to).await
                }
                WatchChange::Delete(path) => coordinator.delete_document(&folder_id, &path),
            };
            if let Err(error) = applied {
                result = Err(error.to_string());
            }
            recently_applied.insert(key, Instant::now());
        }
        recently_applied.retain(|_, applied| applied.elapsed() < DEBOUNCE);
        for waiter in flush_waiters.drain(..) {
            let _ = waiter.send(result.clone());
        }
    }
}

fn queue_change(
    pending: &mut HashMap<String, WatchChange>,
    recently_applied: &HashMap<String, Instant>,
    change: WatchChange,
) {
    let key = change_key(&change);
    if recently_applied
        .get(&key)
        .is_some_and(|applied| applied.elapsed() < DEBOUNCE)
    {
        return;
    }
    pending.insert(key, change);
}

fn changes_from_event(event: Event) -> Vec<WatchChange> {
    match event.kind {
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if event.paths.len() >= 2 => {
            vec![WatchChange::Rename {
                from: event.paths[0].clone(),
                to: event.paths[1].clone(),
            }]
        }
        EventKind::Remove(_) => event.paths.into_iter().map(WatchChange::Delete).collect(),
        EventKind::Create(_) | EventKind::Modify(_) => {
            event.paths.into_iter().map(WatchChange::Write).collect()
        }
        _ => Vec::new(),
    }
}

fn change_key(change: &WatchChange) -> String {
    match change {
        WatchChange::Write(path) => format!("write:{}", normalized_path_key(path)),
        WatchChange::Rename { from, to } => format!(
            "rename:{}:{}",
            normalized_path_key(from),
            normalized_path_key(to)
        ),
        WatchChange::Delete(path) => format!("delete:{}", normalized_path_key(path)),
    }
}

fn normalized_path_key(path: &std::path::Path) -> String {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
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
    #[error("file watcher reconciliation failed: {0}")]
    Indexing(String),
    #[error(transparent)]
    Coordinator(#[from] IndexingError),
}
