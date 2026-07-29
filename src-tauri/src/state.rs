use std::sync::{atomic::AtomicBool, RwLock};

use crate::settings::AppSettings;

pub struct AppState {
    pub settings: RwLock<AppSettings>,
    pub database_ready: AtomicBool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            settings: RwLock::new(AppSettings::default()),
            database_ready: AtomicBool::new(false),
        }
    }
}
