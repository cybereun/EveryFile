pub mod application;
pub mod domain;
pub mod folders;
pub mod infrastructure;
pub mod settings;
pub mod state;

use std::sync::Arc;

use infrastructure::database::Database;
use infrastructure::secure_key::SecureKeyStore;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            let key = SecureKeyStore::load_or_create(&app_data_dir)?;
            let database = Arc::new(Database::open(&app_data_dir.join("everyfile.db"), &key)?);
            database.migrate()?;
            app.manage(state::AppState::new(database));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            application::commands::get_settings,
            application::commands::save_settings,
            application::commands::register_folder,
            application::commands::remove_folder,
            application::commands::list_folders,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
