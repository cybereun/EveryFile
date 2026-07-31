pub mod ai;
pub mod application;
pub mod diagnostics;
pub mod domain;
#[cfg(feature = "e2e")]
mod e2e;
pub mod export;
pub mod folders;
pub mod indexing;
pub mod infrastructure;
pub mod library;
pub mod ocr;
pub mod parsing;
pub mod search;
pub mod settings;
pub mod state;
pub mod statistics;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use indexing::IndexCoordinator;
use infrastructure::database::Database;
use infrastructure::secure_key::SecureKeyStore;
use parsing::ParserClient;
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    run_with_reset_completion(None);
}

pub fn run_with_reset_completion(reset_completion: Option<diagnostics::ResetCompletionStartup>) {
    let reset_completion = Arc::new(Mutex::new(reset_completion));
    let builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init());
    #[cfg(feature = "e2e")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());
    builder
        .setup(move |app| {
            #[cfg(feature = "e2e")]
            if let Some(window) = app.get_webview_window("main") {
                window.set_size(tauri::LogicalSize::new(1440.0, 900.0))?;
            }
            let app_data_dir = app.path().app_local_data_dir()?;
            let key = SecureKeyStore::load_or_create(&app_data_dir)?;
            let database = Arc::new(Database::open(&app_data_dir.join("everyfile.db"), &key)?);
            database.migrate()?;
            let settings_repository = settings::SettingsRepository::new(Arc::clone(&database));
            let loaded_settings = settings_repository.load_with_migration()?;
            let normalized_legacy_settings = loaded_settings.normalized_unsupported_flags;
            let persisted_settings = loaded_settings.settings;
            statistics::StatisticsRepository::new(Arc::clone(&database))
                .run_due_history_retention(persisted_settings.history_retention_days)?;
            let parser = Arc::new(ParserClient::new(
                parser_executable_path(),
                Duration::from_secs(30),
            ));
            let ocr = Arc::new(ocr::OcrClient::new(
                ocr_executable_path(),
                Duration::from_secs(300),
            ));
            let app_handle = app.handle().clone();
            let status_sink = Arc::new(move |status| {
                app_handle
                    .emit("index-status://changed", status)
                    .map_err(|error| error.to_string())
            });
            let indexing = Arc::new(IndexCoordinator::with_parser_ocr_and_sink(
                Arc::clone(&database),
                parser,
                ocr,
                persisted_settings.max_file_size_bytes,
                Some(status_sink),
            ));
            let app_state = state::AppState::new(database, indexing);
            app_state
                .indexing
                .apply_runtime_settings(&persisted_settings)?;
            *app_state
                .settings
                .write()
                .map_err(|_| "settings lock is unavailable")? = persisted_settings;
            tauri::async_runtime::block_on(app_state.restore_runtime())?;
            #[cfg(feature = "e2e")]
            tauri::async_runtime::block_on(e2e::register_startup_fixture(&app_state))?;
            let registered_roots = app_state
                .folders
                .list()?
                .into_iter()
                .map(|folder| PathBuf::from(folder.canonical_path))
                .collect();
            let diagnostics = diagnostics::DiagnosticsLogger::new(&app_data_dir, registered_roots)?;
            if normalized_legacy_settings {
                diagnostics.write(&diagnostics::DiagnosticEvent {
                    level: "info".into(),
                    code: "SETTINGS_LEGACY_FLAGS_NORMALIZED".into(),
                    message:
                        "Unsupported legacy startup, hidden-start, and tray settings were disabled"
                            .into(),
                    document_id: None,
                })?;
            }
            diagnostics.write(&diagnostics::DiagnosticEvent {
                level: "info".into(),
                code: "APP_STARTED".into(),
                message: "EveryFile started; local diagnostic retention completed".into(),
                document_id: None,
            })?;
            app.manage(app_state);
            if let Some(completion) = reset_completion
                .lock()
                .map_err(|_| "reset completion lock is unavailable")?
                .take()
            {
                diagnostics::finish_reset_completion_startup(completion)?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            application::commands::get_settings,
            application::commands::save_settings,
            application::commands::get_ai_secret_status,
            application::commands::save_ai_secret,
            application::commands::run_document_ai,
            application::commands::cancel_document_ai,
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
            application::commands::open_source_location,
            application::commands::get_preview,
            application::commands::get_pdf_bytes,
            application::commands::cancel_pdf_read,
            application::commands::set_bookmark,
            application::commands::remove_bookmark,
            application::commands::create_tag,
            application::commands::set_document_tags,
            application::commands::save_markdown,
            application::commands::get_statistics,
            application::commands::list_search_history,
            application::commands::delete_search_history,
            application::commands::clear_search_history,
            application::commands::export_results,
            application::commands::list_parse_errors,
            application::commands::retry_parse,
            application::commands::reset_application_data,
            application::commands::get_diagnostics_log_folder,
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

fn ocr_executable_path() -> PathBuf {
    let file_name = if cfg!(windows) {
        "everyfile-ocr.exe"
    } else {
        "everyfile-ocr"
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
            "everyfile-ocr-{target}{}",
            if cfg!(windows) { ".exe" } else { "" }
        ))
}
