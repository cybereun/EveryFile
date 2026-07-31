#![cfg(feature = "e2e")]

use std::path::{Path, PathBuf};

use crate::state::AppState;

const FIXTURE_ARGUMENT: &str = "--e2e-register-fixture-folder";

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
    state
        .indexing
        .start(&folder.id)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_fixture_folder;
    use std::path::PathBuf;

    #[test]
    fn rejects_paths_outside_the_fixture_root() {
        let outside = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let error = validate_fixture_folder(&outside).expect_err("must reject source root");
        assert!(error.contains("tests/fixtures"));
    }
}
