use std::collections::HashMap;
use std::sync::{atomic::AtomicBool, Arc, RwLock};

use crate::folders::repository::FolderRepository;
use crate::indexing::{IndexCoordinator, IndexWatcher};
use crate::infrastructure::database::Database;
use crate::settings::AppSettings;

pub struct AppState {
    pub settings: RwLock<AppSettings>,
    pub database_ready: AtomicBool,
    pub database: Arc<Database>,
    pub folders: FolderRepository,
    pub indexing: Arc<IndexCoordinator>,
    pub watchers: tokio::sync::Mutex<HashMap<String, IndexWatcher>>,
}

impl AppState {
    pub fn new(database: Arc<Database>, indexing: Arc<IndexCoordinator>) -> Self {
        Self {
            settings: RwLock::new(AppSettings::default()),
            database_ready: AtomicBool::new(true),
            folders: FolderRepository::new(Arc::clone(&database)),
            database,
            indexing,
            watchers: tokio::sync::Mutex::new(HashMap::new()),
        }
    }
}
