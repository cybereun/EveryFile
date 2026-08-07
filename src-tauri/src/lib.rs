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
pub mod system;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use indexing::IndexCoordinator;
use infrastructure::database::Database;
use infrastructure::secure_key::SecureKeyStore;
use parsing::ParserClient;
use tauri::{Emitter, Manager, WindowEvent};

#[cfg(feature = "tray-icon")]
use tauri::menu::{MenuBuilder, MenuItemBuilder};
#[cfg(feature = "tray-icon")]
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    run_with_reset_completion(None);
}

pub fn run_with_reset_completion(reset_completion: Option<diagnostics::ResetCompletionStartup>) {
    let reset_completion = Arc::new(Mutex::new(reset_completion));
    let builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init());
    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build());
    #[cfg(feature = "e2e")]
    let builder = builder
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());
    builder
        .setup(move |app| {
            #[cfg(feature = "e2e")]
            if let Some(window) = app.get_webview_window("main") {
                window.set_size(tauri::LogicalSize::new(1440.0, 900.0))?;
                window.set_position(tauri::LogicalPosition::new(-10_000.0, -10_000.0))?;
            }
            let app_data_dir = app.path().app_local_data_dir()?;
            #[cfg(feature = "e2e")]
            e2e::reset_state_if_requested(&app_data_dir).map_err(std::io::Error::other)?;
            let key = SecureKeyStore::load_or_create(&app_data_dir)?;
            let database = Arc::new(Database::open(&app_data_dir.join("everyfile.db"), &key)?);
            database.migrate()?;
            let settings_repository = settings::SettingsRepository::new(Arc::clone(&database));
            let loaded_settings = settings_repository.load_with_migration()?;
            let persisted_settings = loaded_settings.settings;
            #[cfg(feature = "e2e")]
            let persisted_settings = {
                let mut settings = persisted_settings;
                e2e::apply_settings_overrides(&mut settings);
                settings
            };
            let start_hidden = persisted_settings.start_hidden;
            let start_with_windows = persisted_settings.start_with_windows;
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
            diagnostics.write(&diagnostics::DiagnosticEvent {
                level: "info".into(),
                code: "APP_STARTED".into(),
                message: "EveryFile started; local diagnostic retention completed".into(),
                document_id: None,
            })?;
            if let Err(error) = system::apply_startup(start_with_windows) {
                let _ = diagnostics.write(&diagnostics::DiagnosticEvent {
                    level: "warn".into(),
                    code: "STARTUP_REGISTRATION_FAILED".into(),
                    message: error.to_string(),
                    document_id: None,
                });
            }
            app.manage(app_state);

            #[cfg(feature = "tray-icon")]
            setup_tray(app)?;

            if start_hidden {
                if let Some(window) = app.get_webview_window("main") {
                    window.hide()?;
                }
            }

            if let Some(window) = app.get_webview_window("main") {
                let app_handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        let minimize_to_tray = app_handle
                            .state::<state::AppState>()
                            .settings
                            .read()
                            .map(|settings| settings.minimize_to_tray)
                            .unwrap_or(false);
                        if minimize_to_tray {
                            api.prevent_close();
                            if let Some(window) = app_handle.get_webview_window("main") {
                                let _ = window.hide();
                            }
                        }
                    }
                });
            }

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
            application::commands::open_folder_location,
            application::commands::get_preview,
            application::commands::get_pdf_bytes,
            application::commands::get_layout_bytes,
            application::commands::get_image_bytes,
            application::commands::cancel_pdf_read,
            application::commands::set_bookmark,
            application::commands::remove_bookmark,
            application::commands::list_bookmarks,
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

#[cfg(feature = "tray-icon")]
fn setup_tray(app: &tauri::App<tauri::Wry>) -> tauri::Result<()> {
    let show = MenuItemBuilder::with_id("show", "EveryFile 열기").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "EveryFile 종료").build(app)?;
    let menu = MenuBuilder::new(app)
        .item(&show)
        .separator()
        .item(&quit)
        .build()?;
    let mut tray = TrayIconBuilder::with_id("everyfile-tray")
        .menu(&menu)
        .tooltip("EveryFile")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    ..
                }
            ) {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }
    tray.build(app)?;
    Ok(())
}

#[cfg(feature = "tray-icon")]
fn show_main_window(app: &tauri::AppHandle<tauri::Wry>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
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
