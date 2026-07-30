use std::path::PathBuf;

use serde::Serialize;
use tauri::AppHandle;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

use crate::application::source_open::{open_indexed_source, SourceOpenError};
use crate::domain::models::{FolderRecord, SearchRequest, SearchResponse};
use crate::folders::repository::{FolderError, FolderRepository};
use crate::indexing::{IndexStatus, IndexingError, JobId};
use crate::search::{SearchError, SearchRepository};
use crate::{settings::AppSettings, state::AppState};

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
    *current_settings = settings;
    Ok(current_settings.clone())
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
    state.folders.remove(&folder_id).map_err(CommandError::from)
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
