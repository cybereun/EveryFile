use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, RwLock,
};

use crate::folders::repository::FolderRepository;
use crate::indexing::{IndexCoordinator, IndexWatcher};
use crate::infrastructure::database::Database;
use crate::library::pdf_read::PdfReadRegistry;
use crate::search::SearchRegistry;
use crate::settings::AppSettings;
use thiserror::Error;

pub struct AppState {
    pub settings: RwLock<AppSettings>,
    pub database_ready: AtomicBool,
    pub database: Arc<Database>,
    pub folders: FolderRepository,
    pub indexing: Arc<IndexCoordinator>,
    pub searches: SearchRegistry,
    pub pdf_reads: PdfReadRegistry,
    pub watchers: tokio::sync::Mutex<HashMap<String, IndexWatcher>>,
}

impl AppState {
    pub fn new(database: Arc<Database>, indexing: Arc<IndexCoordinator>) -> Self {
        let searches = SearchRegistry::new(database.interrupt_handle());
        Self {
            settings: RwLock::new(AppSettings::default()),
            database_ready: AtomicBool::new(true),
            folders: FolderRepository::new(Arc::clone(&database)),
            database,
            indexing,
            searches,
            pdf_reads: PdfReadRegistry::default(),
            watchers: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    pub async fn restore_runtime(&self) -> Result<(), StateError> {
        self.indexing.recover().await?;
        for folder in self
            .folders
            .list()?
            .into_iter()
            .filter(|folder| folder.index_state != "disabled")
        {
            match IndexWatcher::start(Arc::clone(&self.indexing), folder.id.clone()).await {
                Ok(watcher) => {
                    self.watchers.lock().await.insert(folder.id, watcher);
                }
                Err(error) => {
                    self.indexing.record_folder_diagnostic(
                        &folder.id,
                        "WATCHER_RESTORE_FAILED",
                        &error.to_string(),
                    )?;
                }
            }
        }
        Ok(())
    }

    pub async fn activate_registered_folder(&self, folder_id: &str) -> Result<String, StateError> {
        let watcher =
            match IndexWatcher::start(Arc::clone(&self.indexing), folder_id.to_owned()).await {
                Ok(watcher) => watcher,
                Err(error) => {
                    self.folders.remove(folder_id)?;
                    return Err(StateError::Watcher(error));
                }
            };
        let job_id = match self.indexing.start(folder_id).await {
            Ok(job_id) => job_id,
            Err(error) => {
                drop(watcher);
                self.folders.remove(folder_id)?;
                return Err(StateError::Indexing(error));
            }
        };
        self.watchers
            .lock()
            .await
            .insert(folder_id.to_owned(), watcher);
        Ok(job_id)
    }

    pub async fn prepare_for_reset(&self) -> Result<(), StateError> {
        self.database_ready.store(false, Ordering::Release);
        self.searches.cancel_all();
        self.pdf_reads.cancel_all();
        self.watchers.lock().await.clear();
        self.indexing.shutdown_all().await;
        self.database.close()?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum StateError {
    #[error(transparent)]
    Folder(#[from] crate::folders::repository::FolderError),
    #[error(transparent)]
    Indexing(#[from] crate::indexing::IndexingError),
    #[error(transparent)]
    Watcher(#[from] crate::indexing::WatcherError),
    #[error(transparent)]
    Database(#[from] crate::infrastructure::database::DatabaseError),
}
