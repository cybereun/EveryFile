use std::path::PathBuf;

use serde::Serialize;
use tauri::AppHandle;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

use crate::domain::models::FolderRecord;
use crate::folders::repository::{FolderError, FolderRepository};
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

    register_selected_folder(selected_path, &state.folders).map_err(CommandError::from)
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
pub fn remove_folder(folder_id: String, state: State<'_, AppState>) -> Result<(), CommandError> {
    state.folders.remove(&folder_id).map_err(CommandError::from)
}

#[tauri::command]
pub fn list_folders(state: State<'_, AppState>) -> Result<Vec<FolderRecord>, CommandError> {
    state.folders.list().map_err(CommandError::from)
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
