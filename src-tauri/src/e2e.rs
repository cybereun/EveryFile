#![cfg(feature = "e2e")]

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::state::AppState;

const FIXTURE_ARGUMENT: &str = "--e2e-register-fixture-folder";
const ENABLE_OCR_ARGUMENT: &str = "--e2e-enable-ocr";
const RESET_STATE_ARGUMENT: &str = "--e2e-reset-state";
const DATA_DIR_ARGUMENT: &str = "--e2e-data-dir";

/// Returns the dedicated, temporary data directory required for an E2E run.
///
/// The E2E executable must never fall back to the normal application data
/// directory: its reset flag deliberately deletes the index database.
pub fn data_dir_from_args() -> Result<PathBuf, String> {
    data_dir_from_values(std::env::args_os())
}

fn data_dir_from_values<I>(arguments: I) -> Result<PathBuf, String>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = arguments.into_iter();
    while let Some(argument) = args.next() {
        if argument == DATA_DIR_ARGUMENT {
            let value = args
                .next()
                .ok_or_else(|| format!("{DATA_DIR_ARGUMENT} requires a directory"))?;
            return validate_data_dir(Path::new(&value));
        }
    }
    Err(format!(
        "{DATA_DIR_ARGUMENT} is required for every E2E application launch"
    ))
}

fn validate_data_dir(path: &Path) -> Result<PathBuf, String> {
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("invalid E2E data directory: {error}"))?;
    if !canonical.is_dir() {
        return Err("E2E data directory must be a directory".into());
    }
    let temporary_root = std::env::temp_dir()
        .canonicalize()
        .map_err(|error| format!("temporary directory is unavailable: {error}"))?;
    if canonical == temporary_root || !canonical.starts_with(&temporary_root) {
        return Err(
            "E2E data directory must be a dedicated child of the temporary directory".into(),
        );
    }
    Ok(canonical)
}

pub fn reset_state_if_requested(app_data_dir: &Path) -> Result<(), String> {
    if !std::env::args().any(|argument| argument == RESET_STATE_ARGUMENT) {
        return Ok(());
    }
    for name in ["everyfile.db", "everyfile.db-wal", "everyfile.db-shm"] {
        let path = app_data_dir.join(name);
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|error| format!("failed to reset {}: {error}", path.display()))?;
        }
    }
    Ok(())
}

pub fn apply_settings_overrides(settings: &mut crate::domain::models::AppSettings) {
    if std::env::args().any(|argument| argument == ENABLE_OCR_ARGUMENT) {
        settings.ocr_enabled = true;
        settings.math_ocr_enabled = false;
        settings.ai_enabled = false;
    }
}

pub fn fixture_folder_from_args() -> Result<Option<PathBuf>, String> {
    let mut args = std::env::args_os();
    while let Some(argument) = args.next() {
        if argument == FIXTURE_ARGUMENT {
            let value = args
                .next()
                .ok_or_else(|| format!("{FIXTURE_ARGUMENT} requires a path"))?;
            return validate_fixture_folder(Path::new(&value)).map(Some);
        }
    }
    Ok(None)
}

fn validate_fixture_folder(path: &Path) -> Result<PathBuf, String> {
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("invalid E2E fixture folder: {error}"))?;
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "workspace root is unavailable".to_string())?
        .join("tests")
        .join("fixtures")
        .canonicalize()
        .map_err(|error| format!("E2E fixture root is unavailable: {error}"))?;
    if !canonical.is_dir() || !canonical.starts_with(&fixture_root) {
        return Err("E2E folder must be a directory under tests/fixtures".into());
    }
    Ok(canonical)
}

pub async fn register_startup_fixture(state: &AppState) -> Result<(), String> {
    let Some(path) = fixture_folder_from_args()? else {
        return Ok(());
    };
    let canonical = path.to_string_lossy().into_owned();
    let folder = match state.folders.register(&path) {
        Ok(folder) => folder,
        Err(_) => state
            .folders
            .list()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|folder| folder.canonical_path == canonical)
            .ok_or_else(|| "fixture folder could not be registered".to_string())?,
    };
    state
        .activate_registered_folder(&folder.id)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        data_dir_from_values, reset_state_if_requested, validate_fixture_folder, DATA_DIR_ARGUMENT,
    };
    use std::ffi::OsString;
    use std::path::PathBuf;

    #[test]
    fn accepts_a_dedicated_temporary_data_directory() {
        let directory = tempfile::tempdir().expect("temp directory");
        let expected = directory
            .path()
            .canonicalize()
            .expect("canonical temp directory");
        let actual = data_dir_from_values([
            OsString::from("EveryFile.exe"),
            OsString::from(DATA_DIR_ARGUMENT),
            directory.path().as_os_str().to_os_string(),
        ])
        .expect("dedicated temporary data directory");
        assert_eq!(actual, expected);
    }

    #[test]
    fn rejects_a_missing_e2e_data_directory() {
        let error = data_dir_from_values([OsString::from("EveryFile.exe")])
            .expect_err("E2E launch must provide an isolated data directory");
        assert!(error.contains(DATA_DIR_ARGUMENT));
    }

    #[test]
    fn rejects_a_data_directory_outside_temp() {
        let outside = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let error = data_dir_from_values([
            OsString::from("EveryFile.exe"),
            OsString::from(DATA_DIR_ARGUMENT),
            outside.into_os_string(),
        ])
        .expect_err("source directory must not be usable as E2E data");
        assert!(error.contains("dedicated child"));
    }

    #[test]
    fn rejects_paths_outside_the_fixture_root() {
        let outside = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let error = validate_fixture_folder(&outside).expect_err("must reject source root");
        assert!(error.contains("tests/fixtures"));
    }

    #[test]
    fn reset_is_inert_without_the_private_e2e_argument() {
        let directory = tempfile::tempdir().expect("temp directory");
        let database = directory.path().join("everyfile.db");
        std::fs::write(&database, b"keep").expect("fixture");
        reset_state_if_requested(directory.path()).expect("reset check");
        assert!(database.exists());
    }
}
