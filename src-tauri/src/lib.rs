pub mod application;
pub mod domain;
pub mod folders;
pub mod indexing;
pub mod infrastructure;
pub mod parsing;
pub mod search;
pub mod settings;
pub mod state;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use indexing::IndexCoordinator;
use infrastructure::database::Database;
use infrastructure::secure_key::SecureKeyStore;
use parsing::ParserClient;
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            let key = SecureKeyStore::load_or_create(&app_data_dir)?;
            let database = Arc::new(Database::open(&app_data_dir.join("everyfile.db"), &key)?);
            database.migrate()?;
            let parser = Arc::new(ParserClient::new(
                parser_executable_path(),
                Duration::from_secs(30),
            ));
            let app_handle = app.handle().clone();
            let status_sink = Arc::new(move |status| {
                app_handle
                    .emit("index-status://changed", status)
                    .map_err(|error| error.to_string())
            });
            let indexing = Arc::new(IndexCoordinator::with_parser_and_sink(
                Arc::clone(&database),
                parser,
                200 * 1024 * 1024,
                Some(status_sink),
            ));
            let app_state = state::AppState::new(database, indexing);
            tauri::async_runtime::block_on(app_state.restore_runtime())?;
            app.manage(app_state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            application::commands::get_settings,
            application::commands::save_settings,
            application::commands::register_folder,
            application::commands::remove_folder,
            application::commands::list_folders,
            application::commands::start_indexing,
            application::commands::pause_indexing,
            application::commands::resume_indexing,
            application::commands::cancel_indexing,
            application::commands::get_index_status,
            application::commands::search_documents,
            application::commands::cancel_search,
            application::commands::open_source_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn parser_executable_path() -> PathBuf {
    let file_name = if cfg!(windows) {
        "everyfile-parser.exe"
    } else {
        "everyfile-parser"
    };
    let packaged = std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(|parent| parent.join(file_name)));
    if let Some(path) = packaged.filter(|path| path.is_file()) {
        return path;
    }

    let target = option_env!("TAURI_ENV_TARGET_TRIPLE").unwrap_or("x86_64-pc-windows-msvc");
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join(format!(
            "everyfile-parser-{target}{}",
            if cfg!(windows) { ".exe" } else { "" }
        ))
}
