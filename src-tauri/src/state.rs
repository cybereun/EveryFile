use std::sync::{atomic::AtomicBool, Arc, RwLock};

use crate::folders::repository::FolderRepository;
use crate::infrastructure::database::Database;
use crate::settings::AppSettings;

pub struct AppState {
    pub settings: RwLock<AppSettings>,
    pub database_ready: AtomicBool,
    pub database: Arc<Database>,
    pub folders: FolderRepository,
}

impl AppState {
    pub fn new(database: Arc<Database>) -> Self {
        Self {
            settings: RwLock::new(AppSettings::default()),
            database_ready: AtomicBool::new(true),
            folders: FolderRepository::new(Arc::clone(&database)),
            database,
        }
    }
}
