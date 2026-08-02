use std::path::PathBuf;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use tauri::State;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;
use zeroize::Zeroizing;

use crate::ai::AiService;
use crate::application::source_open::{
    open_indexed_location, open_indexed_source, open_registered_folder,
    read_indexed_image_cancellable, read_indexed_layout_cancellable, read_indexed_pdf_cancellable,
    SourceOpenError,
};
use crate::diagnostics::{
    require_reset_confirmation, start_reset_worker, DiagnosticError, DiagnosticsLogger,
};
use crate::domain::models::{
    BookmarkRecord, BookmarkSummary, FolderRecord, PreviewDocument, SearchRequest, SearchResponse,
    TagRecord,
};
use crate::export::{
    export_to_destination, ExportError, ExportFormat, ExportOutcome, ExportRequest,
};
use crate::folders::repository::{FolderError, FolderRepository};
use crate::indexing::{IndexStatus, IndexingError, JobId};
use crate::library::pdf_read::PdfReadError;
use crate::library::repository::{LibraryError, LibraryRepository};
use crate::search::{SearchError, SearchRepository};
use crate::settings::{AppSettings, SettingsError, SettingsRepository};
use crate::state::AppState;
use crate::statistics::{
    DocumentStatistics, ParseErrorRecord, SearchHistoryRecord, StatisticsError,
    StatisticsRepository,
};

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    state
        .settings
        .read()
        .map(|settings| settings.clone())
        .map_err(|_| "settings lock is unavailable".into())
}

#[tauri::command]
pub fn save_settings(
    settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<AppSettings, String> {
    let mut current_settings = state
        .settings
        .write()
        .map_err(|_| "settings lock is unavailable".to_string())?;
    let saved = SettingsRepository::new(state.database.clone())
        .save(&settings)
        .map_err(|error| error.to_string())?;
    StatisticsRepository::new(state.database.clone())
        .run_history_retention(saved.history_retention_days)
        .map_err(|error| error.to_string())?;
    state
        .indexing
        .apply_runtime_settings(&saved)
        .map_err(|error| error.to_string())?;
    crate::system::apply_startup(saved.start_with_windows).map_err(|error| error.to_string())?;
    *current_settings = saved;
    Ok(current_settings.clone())
}

fn validate_secret_provider(provider: &str) -> Result<(), CommandError> {
    if matches!(provider, "gemini" | "openai") {
        Ok(())
    } else {
        Err(CommandError::new(
            "AI_PROVIDER_INVALID",
            "API secrets are supported only for Gemini and OpenAI",
        ))
    }
}

#[tauri::command]
pub fn get_ai_secret_status(
    provider: String,
    state: State<'_, AppState>,
) -> Result<bool, CommandError> {
    validate_secret_provider(&provider)?;
    let present = state
        .database
        .connection()
        .query_row(
            "SELECT 1 FROM ai_secrets WHERE provider = ?1",
            params![provider],
            |_| Ok(true),
        )
        .optional()
        .map_err(|error| CommandError::new("AI_SECRET_READ_FAILED", error.to_string()))?
        .unwrap_or(false);
    Ok(present)
}

#[tauri::command]
pub fn save_ai_secret(
    provider: String,
    secret: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    validate_secret_provider(&provider)?;
    let normalized = secret.map(|value| value.trim().to_string());
    if normalized.as_ref().is_some_and(|value| value.len() > 4096) {
        return Err(CommandError::new(
            "AI_SECRET_INVALID",
            "API key is too long",
        ));
    }
    let connection = state.database.connection();
    match normalized.filter(|value| !value.is_empty()) {
        Some(value) => {
            let value = Zeroizing::new(value);
            connection
                .execute(
                    "INSERT INTO ai_secrets (provider, secret, updated_at)
                     VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                     ON CONFLICT(provider) DO UPDATE SET
                       secret = excluded.secret,
                       updated_at = excluded.updated_at",
                    params![provider, value.as_str()],
                )
                .map_err(|error| CommandError::new("AI_SECRET_SAVE_FAILED", error.to_string()))?;
        }
        None => {
            connection
                .execute(
                    "DELETE FROM ai_secrets WHERE provider = ?1",
                    params![provider],
                )
                .map_err(|error| CommandError::new("AI_SECRET_DELETE_FAILED", error.to_string()))?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn run_document_ai(
    request_id: String,
    document_id: String,
    question: Option<String>,
    remote_consent: bool,
    state: State<'_, AppState>,
) -> Result<String, CommandError> {
    let settings = state
        .settings
        .read()
        .map_err(|_| CommandError::new("SETTINGS_LOCK_FAILED", "settings are unavailable"))?
        .clone();
    AiService::new(state.database.clone(), &state.ai_requests)
        .run(
            &request_id,
            &document_id,
            question.as_deref(),
            &settings,
            remote_consent,
        )
        .await
        .map_err(|error| CommandError::new(error.code(), error.to_string()))
}

#[tauri::command]
pub fn cancel_document_ai(
    request_id: String,
    state: State<'_, AppState>,
) -> Result<bool, CommandError> {
    if request_id.is_empty() || request_id.len() > 100 {
        return Err(CommandError::new(
            "AI_REQUEST_INVALID",
            "AI request id is invalid",
        ));
    }
    Ok(state.ai_requests.cancel(&request_id))
}

#[tauri::command]
pub async fn register_folder(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<FolderRecord>, CommandError> {
    let selected_path = app
        .dialog()
        .file()
        .blocking_pick_folder()
        .map(|selected| {
            selected
                .into_path()
                .map_err(|error| CommandError::new("FOLDER_PICKER_PATH_INVALID", error.to_string()))
        })
        .transpose()?;

    let registered =
        register_selected_folder(selected_path, &state.folders).map_err(CommandError::from)?;
    if let Some(folder) = &registered {
        state
            .activate_registered_folder(&folder.id)
            .await
            .map_err(|error| CommandError::new("FOLDER_ACTIVATION_FAILED", error.to_string()))?;
    }
    Ok(registered)
}

pub fn register_selected_folder(
    selected_path: Option<PathBuf>,
    repository: &FolderRepository,
) -> Result<Option<FolderRecord>, FolderError> {
    selected_path
        .map(|path| repository.register(&path))
        .transpose()
}

#[tauri::command]
pub async fn remove_folder(
    folder_id: String,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    state.watchers.lock().await.remove(&folder_id);
    state
        .indexing
        .cancel_for_folder(&folder_id)
        .await
        .map_err(CommandError::from)?;
    // Folder removal can delete thousands of documents, FTS rows, and
    // cascading library records. Keep that synchronous SQLite transaction off
    // the async/Tauri runtime thread so the window remains responsive.
    let repository = state.folders.clone();
    tokio::task::spawn_blocking(move || repository.remove(&folder_id))
        .await
        .map_err(|error| CommandError::new("FOLDER_REMOVE_WORKER_FAILED", error.to_string()))?
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn list_folders(state: State<'_, AppState>) -> Result<Vec<FolderRecord>, CommandError> {
    state.folders.list().map_err(CommandError::from)
}

#[tauri::command]
pub async fn start_indexing(
    folder_id: String,
    state: State<'_, AppState>,
) -> Result<JobId, CommandError> {
    state
        .indexing
        .start(&folder_id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn pause_indexing(
    job_id: String,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    state
        .indexing
        .pause(&job_id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn resume_indexing(
    job_id: String,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    state
        .indexing
        .resume(&job_id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn cancel_indexing(
    job_id: String,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    state
        .indexing
        .cancel(&job_id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn get_index_status(
    job_id: String,
    state: State<'_, AppState>,
) -> Result<IndexStatus, CommandError> {
    state
        .indexing
        .status(&job_id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn search_documents(
    request: SearchRequest,
    state: State<'_, AppState>,
) -> Result<SearchResponse, CommandError> {
    let repository = SearchRepository::new(
        state.database.clone(),
        state.indexing.activity_limiter(),
        state.searches.clone(),
    );
    let lease = repository
        .begin_request(&request.request_id)
        .map_err(CommandError::from)?;
    tauri::async_runtime::spawn_blocking(move || repository.search_registered(&request, lease))
        .await
        .map_err(|error| CommandError::new("SEARCH_WORKER_FAILED", error.to_string()))?
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn cancel_search(request_id: String, state: State<'_, AppState>) -> bool {
    state.searches.cancel(&request_id)
}

#[tauri::command]
pub fn open_source_file(
    document_id: String,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    open_indexed_source(&state.database, &document_id).map_err(CommandError::from)
}

#[tauri::command]
pub fn open_source_location(
    document_id: String,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    open_indexed_location(&state.database, &document_id).map_err(CommandError::from)
}

#[tauri::command]
pub fn open_folder_location(
    folder_id: String,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    open_registered_folder(&state.database, &folder_id).map_err(CommandError::from)
}

#[tauri::command]
pub fn get_preview(
    document_id: String,
    state: State<'_, AppState>,
) -> Result<PreviewDocument, CommandError> {
    LibraryRepository::new(state.database.clone())
        .get_preview(&document_id)
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn get_pdf_bytes(
    document_id: String,
    request_id: String,
    state: State<'_, AppState>,
) -> Result<tauri::ipc::Response, CommandError> {
    let lease = state
        .pdf_reads
        .begin(&request_id)
        .map_err(CommandError::from)?;
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = read_indexed_pdf_cancellable(&database, &document_id, || lease.is_cancelled())
            .map_err(CommandError::from)?;
        lease
            .finish(bytes)
            .map(tauri::ipc::Response::new)
            .map_err(CommandError::from)
    })
    .await
    .map_err(|error| CommandError::new("PDF_READ_WORKER_FAILED", error.to_string()))?
}

#[tauri::command]
pub async fn get_layout_bytes(
    document_id: String,
    request_id: String,
    state: State<'_, AppState>,
) -> Result<tauri::ipc::Response, CommandError> {
    let lease = state
        .pdf_reads
        .begin(&request_id)
        .map_err(CommandError::from)?;
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let bytes =
            read_indexed_layout_cancellable(&database, &document_id, || lease.is_cancelled())
                .map_err(CommandError::from)?;
        lease
            .finish(bytes)
            .map(tauri::ipc::Response::new)
            .map_err(CommandError::from)
    })
    .await
    .map_err(|error| CommandError::new("LAYOUT_READ_WORKER_FAILED", error.to_string()))?
}

#[tauri::command]
pub async fn get_image_bytes(
    document_id: String,
    request_id: String,
    state: State<'_, AppState>,
) -> Result<tauri::ipc::Response, CommandError> {
    let lease = state
        .pdf_reads
        .begin(&request_id)
        .map_err(CommandError::from)?;
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = read_indexed_image_cancellable(&database, &document_id, || lease.is_cancelled())
            .map_err(CommandError::from)?;
        lease
            .finish(bytes)
            .map(tauri::ipc::Response::new)
            .map_err(CommandError::from)
    })
    .await
    .map_err(|error| CommandError::new("IMAGE_READ_WORKER_FAILED", error.to_string()))?
}

#[tauri::command]
pub fn cancel_pdf_read(request_id: String, state: State<'_, AppState>) -> bool {
    state.pdf_reads.cancel(&request_id)
}

#[tauri::command]
pub fn set_bookmark(
    document_id: String,
    note: String,
    state: State<'_, AppState>,
) -> Result<BookmarkRecord, CommandError> {
    LibraryRepository::new(state.database.clone())
        .set_bookmark(&document_id, &note)
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn remove_bookmark(
    document_id: String,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    LibraryRepository::new(state.database.clone())
        .remove_bookmark(&document_id)
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn list_bookmarks(state: State<'_, AppState>) -> Result<Vec<BookmarkSummary>, CommandError> {
    LibraryRepository::new(state.database.clone())
        .list_bookmarks()
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn create_tag(
    name: String,
    color: String,
    state: State<'_, AppState>,
) -> Result<TagRecord, CommandError> {
    LibraryRepository::new(state.database.clone())
        .create_tag(&name, &color)
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn set_document_tags(
    document_id: String,
    tag_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<TagRecord>, CommandError> {
    LibraryRepository::new(state.database.clone())
        .set_document_tags(&document_id, &tag_ids)
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn save_markdown(
    app: AppHandle,
    document_id: String,
    state: State<'_, AppState>,
) -> Result<bool, CommandError> {
    let (file_name, markdown) = LibraryRepository::new(state.database.clone())
        .markdown(&document_id)
        .map_err(CommandError::from)?;
    let suggested_name = markdown_file_name(&file_name);
    let selected = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("Markdown", &["md"])
            .set_file_name(suggested_name)
            .blocking_save_file()
    })
    .await
    .map_err(|error| CommandError::new("MARKDOWN_DIALOG_FAILED", error.to_string()))?;
    let Some(selected) = selected else {
        return Ok(false);
    };
    let path = selected
        .into_path()
        .map_err(|error| CommandError::new("MARKDOWN_PATH_INVALID", error.to_string()))?;
    std::fs::write(path, markdown)
        .map_err(|error| CommandError::new("MARKDOWN_SAVE_FAILED", error.to_string()))?;
    Ok(true)
}

#[tauri::command]
pub fn get_statistics(state: State<'_, AppState>) -> Result<DocumentStatistics, CommandError> {
    let repository = StatisticsRepository::new(state.database.clone());
    run_due_retention(&repository, &state)?;
    repository.get_statistics().map_err(CommandError::from)
}

#[tauri::command]
pub fn list_search_history(
    limit: u32,
    offset: u32,
    state: State<'_, AppState>,
) -> Result<Vec<SearchHistoryRecord>, CommandError> {
    let repository = StatisticsRepository::new(state.database.clone());
    run_due_retention(&repository, &state)?;
    repository
        .list_search_history(limit, offset)
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn delete_search_history(id: String, state: State<'_, AppState>) -> Result<bool, CommandError> {
    StatisticsRepository::new(state.database.clone())
        .delete_search_history(&id)
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn clear_search_history(state: State<'_, AppState>) -> Result<u64, CommandError> {
    StatisticsRepository::new(state.database.clone())
        .clear_search_history()
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn export_results(
    app: AppHandle,
    request: ExportRequest,
    format: ExportFormat,
) -> Result<ExportOutcome, CommandError> {
    let mut dialog = app.dialog().file();
    dialog = match format {
        ExportFormat::Csv => dialog.add_filter("CSV", &["csv"]),
        ExportFormat::Xlsx => dialog.add_filter("Excel", &["xlsx"]),
        ExportFormat::Markdown => dialog.add_filter("Markdown", &["md"]),
    };
    let suggested_name = match &request {
        ExportRequest::SearchResults { .. } => match format {
            ExportFormat::Csv => "everyfile-search.csv".to_owned(),
            ExportFormat::Xlsx => "everyfile-search.xlsx".to_owned(),
            ExportFormat::Markdown => "everyfile-search.md".to_owned(),
        },
        ExportRequest::MarkdownDocument { file_name, .. } => markdown_file_name(file_name),
    };
    let selected = tauri::async_runtime::spawn_blocking(move || {
        dialog.set_file_name(suggested_name).blocking_save_file()
    })
    .await
    .map_err(|error| CommandError::new("EXPORT_DIALOG_FAILED", error.to_string()))?;
    let selected = selected
        .map(|path| {
            path.into_path()
                .map_err(|error| CommandError::new("EXPORT_PATH_INVALID", error.to_string()))
        })
        .transpose()?;
    export_to_destination(&request, format, selected.as_deref()).map_err(CommandError::from)
}

#[tauri::command]
pub fn list_parse_errors(
    state: State<'_, AppState>,
) -> Result<Vec<ParseErrorRecord>, CommandError> {
    StatisticsRepository::new(state.database.clone())
        .list_parse_errors()
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn retry_parse(
    document_id: String,
    state: State<'_, AppState>,
) -> Result<bool, CommandError> {
    let repository = StatisticsRepository::new(state.database.clone());
    let folder_id = repository
        .failed_document_folder(&document_id)
        .map_err(CommandError::from)?;
    let Some(folder_id) = folder_id else {
        return Ok(false);
    };
    if !repository
        .retry_parse(&document_id)
        .map_err(CommandError::from)?
    {
        return Ok(false);
    }
    state
        .indexing
        .reconcile(&folder_id)
        .await
        .map_err(CommandError::from)?;
    Ok(true)
}

#[tauri::command]
pub async fn reset_application_data(
    app: AppHandle,
    confirmed: bool,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    require_reset_confirmation(confirmed).map_err(CommandError::from)?;
    let app_data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|error| CommandError::new("APP_DATA_PATH_FAILED", error.to_string()))?;
    state
        .prepare_for_reset()
        .await
        .map_err(|error| CommandError::new("RESET_QUIESCE_FAILED", error.to_string()))?;
    if let Err(error) = start_reset_worker(&app_data_dir) {
        app.exit(1);
        return Err(CommandError::from(error));
    }
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub fn get_diagnostics_log_folder(app: AppHandle) -> Result<String, CommandError> {
    let app_data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|error| CommandError::new("APP_DATA_PATH_FAILED", error.to_string()))?;
    let logger = DiagnosticsLogger::new(&app_data_dir, Vec::new()).map_err(CommandError::from)?;
    Ok(logger.log_directory().to_string_lossy().into_owned())
}

fn run_due_retention(
    repository: &StatisticsRepository,
    state: &State<'_, AppState>,
) -> Result<(), CommandError> {
    let days = state
        .settings
        .read()
        .map_err(|_| CommandError::new("SETTINGS_LOCK_FAILED", "settings lock is unavailable"))?
        .history_retention_days;
    repository
        .run_due_history_retention(days)
        .map(|_| ())
        .map_err(CommandError::from)
}

fn markdown_file_name(file_name: &str) -> String {
    let stem = std::path::Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("document");
    let safe = stem
        .chars()
        .filter(|character| !r#"<>:"/\|?*"#.contains(*character) && !character.is_control())
        .take(120)
        .collect::<String>();
    format!(
        "{}.md",
        if safe.trim().is_empty() {
            "document"
        } else {
            &safe
        }
    )
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    code: String,
    message: String,
}

impl CommandError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl From<FolderError> for CommandError {
    fn from(error: FolderError) -> Self {
        Self::new(error.code(), error.to_string())
    }
}

impl From<IndexingError> for CommandError {
    fn from(error: IndexingError) -> Self {
        Self::new(error.code(), error.to_string())
    }
}

impl From<SearchError> for CommandError {
    fn from(error: SearchError) -> Self {
        Self::new(error.code(), error.to_string())
    }
}

impl From<SourceOpenError> for CommandError {
    fn from(error: SourceOpenError) -> Self {
        Self::new(error.code(), error.to_string())
    }
}

impl From<LibraryError> for CommandError {
    fn from(error: LibraryError) -> Self {
        Self::new(error.code(), error.to_string())
    }
}

impl From<PdfReadError> for CommandError {
    fn from(error: PdfReadError) -> Self {
        Self::new(error.code(), error.to_string())
    }
}

impl From<StatisticsError> for CommandError {
    fn from(error: StatisticsError) -> Self {
        Self::new(error.code(), error.to_string())
    }
}

impl From<ExportError> for CommandError {
    fn from(error: ExportError) -> Self {
        Self::new(error.code(), error.to_string())
    }
}

impl From<SettingsError> for CommandError {
    fn from(error: SettingsError) -> Self {
        Self::new(error.code(), error.to_string())
    }
}

impl From<DiagnosticError> for CommandError {
    fn from(error: DiagnosticError) -> Self {
        Self::new(error.code(), error.to_string())
    }
}
