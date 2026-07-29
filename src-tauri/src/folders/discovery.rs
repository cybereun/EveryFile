use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use ignore::{DirEntry, WalkBuilder};
use serde::Serialize;
use thiserror::Error;

use crate::domain::models::FolderRecord;

const DEFAULT_EXCLUDED_DIRECTORIES: &[&str] = &[".git", "node_modules", "$RECYCLE.BIN"];

#[derive(Debug, Clone)]
pub struct DiscoveryOptions {
    excluded_directories: HashSet<String>,
}

impl Default for DiscoveryOptions {
    fn default() -> Self {
        Self {
            excluded_directories: DEFAULT_EXCLUDED_DIRECTORIES
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
        }
    }
}

impl DiscoveryOptions {
    pub fn with_excluded_directory(mut self, name: impl Into<String>) -> Self {
        self.excluded_directories.insert(name.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileCandidate {
    pub canonical_path: PathBuf,
    pub relative_path: String,
    pub size_bytes: u64,
    pub modified_at: Option<SystemTime>,
    pub metadata_only: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryWarning {
    pub code: String,
    pub path: Option<String>,
    pub message: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct DiscoveryReport {
    pub files: Vec<FileCandidate>,
    pub warnings: Vec<DiscoveryWarning>,
}

pub struct DiscoveryStream {
    candidates: std::vec::IntoIter<FileCandidate>,
    warnings: Vec<DiscoveryWarning>,
}

impl DiscoveryStream {
    pub fn warnings(&self) -> &[DiscoveryWarning] {
        &self.warnings
    }
}

impl Iterator for DiscoveryStream {
    type Item = FileCandidate;

    fn next(&mut self) -> Option<Self::Item> {
        self.candidates.next()
    }
}

pub fn discover(
    folder: &FolderRecord,
    options: DiscoveryOptions,
) -> Result<DiscoveryStream, DiscoveryError> {
    let report = discover_all(Path::new(&folder.canonical_path), options)?;
    Ok(DiscoveryStream {
        candidates: report.files.into_iter(),
        warnings: report.warnings,
    })
}

pub fn discover_all(
    selected_root: &Path,
    options: DiscoveryOptions,
) -> Result<DiscoveryReport, DiscoveryError> {
    let canonical_root =
        selected_root
            .canonicalize()
            .map_err(|source| DiscoveryError::RootUnavailable {
                path: selected_root.to_path_buf(),
                source,
            })?;
    if !canonical_root.is_dir() {
        return Err(DiscoveryError::RootIsNotDirectory(canonical_root));
    }

    let excluded_directories = Arc::new(options.excluded_directories);
    let filter_exclusions = Arc::clone(&excluded_directories);
    let mut builder = WalkBuilder::new(&canonical_root);
    builder
        .hidden(false)
        .follow_links(false)
        .ignore(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .parents(false)
        .sort_by_file_path(|left, right| left.cmp(right))
        .filter_entry(move |entry| should_descend(entry, &filter_exclusions));

    let mut report = DiscoveryReport::default();
    for result in builder.build() {
        let entry = match result {
            Ok(entry) => entry,
            Err(error) => {
                report.warnings.push(walk_warning(&error));
                continue;
            }
        };
        if entry.depth() == 0 {
            continue;
        }

        let path = entry.path();
        let link_metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) => {
                report
                    .warnings
                    .push(io_warning("ENTRY_METADATA_UNAVAILABLE", path, error));
                continue;
            }
        };
        if link_metadata.file_type().is_symlink() || !link_metadata.is_file() {
            continue;
        }

        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) => {
                report
                    .warnings
                    .push(io_warning("ENTRY_METADATA_UNAVAILABLE", path, error));
                continue;
            }
        };
        let metadata_only = is_metadata_only(&metadata);
        let canonical_path = match canonical_candidate_path(path, metadata_only) {
            Ok(path) => path,
            Err(error) => {
                report
                    .warnings
                    .push(io_warning("ENTRY_CANONICALIZE_FAILED", path, error));
                continue;
            }
        };
        if !canonical_path.starts_with(&canonical_root) {
            report.warnings.push(DiscoveryWarning {
                code: "ENTRY_OUTSIDE_ROOT".into(),
                path: Some(path.to_string_lossy().into_owned()),
                message: "candidate resolved outside the registered root".into(),
            });
            continue;
        }

        let relative_path = match canonical_path.strip_prefix(&canonical_root) {
            Ok(relative) => relative.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        report.files.push(FileCandidate {
            canonical_path,
            relative_path,
            size_bytes: metadata.len(),
            modified_at: metadata.modified().ok(),
            metadata_only,
        });
    }

    Ok(report)
}

fn canonical_candidate_path(path: &Path, metadata_only: bool) -> std::io::Result<PathBuf> {
    if !metadata_only {
        return path.canonicalize();
    }

    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "metadata-only candidate has no parent",
        )
    })?;
    let file_name = path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "metadata-only candidate has no file name",
        )
    })?;
    Ok(parent.canonicalize()?.join(file_name))
}

fn should_descend(entry: &DirEntry, exclusions: &HashSet<String>) -> bool {
    if entry.depth() == 0 {
        return true;
    }

    let is_directory = entry.file_type().is_some_and(|kind| kind.is_dir());
    if is_directory
        && entry.file_name().to_str().is_some_and(|name| {
            exclusions
                .iter()
                .any(|excluded| name.eq_ignore_ascii_case(excluded))
        })
    {
        return false;
    }

    !is_directory_reparse_point(entry.path())
}

#[cfg(windows)]
fn is_directory_reparse_point(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

    let Ok(link_metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if link_metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
        return false;
    }

    link_metadata.is_dir() || fs::metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

#[cfg(not(windows))]
fn is_directory_reparse_point(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
}

#[cfg(windows)]
fn is_metadata_only(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    is_metadata_only_file_attributes(metadata.file_attributes())
}

#[cfg(not(windows))]
fn is_metadata_only(_metadata: &fs::Metadata) -> bool {
    false
}

#[cfg(windows)]
pub fn is_metadata_only_file_attributes(attributes: u32) -> bool {
    const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
    const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
    const PLACEHOLDER_ATTRIBUTES: u32 = FILE_ATTRIBUTE_OFFLINE
        | FILE_ATTRIBUTE_RECALL_ON_OPEN
        | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS;

    attributes & PLACEHOLDER_ATTRIBUTES != 0
}

fn walk_warning(error: &ignore::Error) -> DiscoveryWarning {
    DiscoveryWarning {
        code: "ENTRY_INACCESSIBLE".into(),
        path: walk_error_path(error).map(|path| path.to_string_lossy().into_owned()),
        message: error.to_string(),
    }
}

fn walk_error_path(error: &ignore::Error) -> Option<&Path> {
    match error {
        ignore::Error::WithPath { path, .. } => Some(path),
        ignore::Error::WithLineNumber { err, .. } | ignore::Error::WithDepth { err, .. } => {
            walk_error_path(err)
        }
        ignore::Error::Loop { child, .. } => Some(child),
        ignore::Error::Partial(errors) => errors.iter().find_map(walk_error_path),
        _ => None,
    }
}

fn io_warning(code: &str, path: &Path, error: std::io::Error) -> DiscoveryWarning {
    DiscoveryWarning {
        code: code.into(),
        path: Some(path.to_string_lossy().into_owned()),
        message: error.to_string(),
    }
}

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("registered root is unavailable: {path}")]
    RootUnavailable {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("registered root is not a directory: {0}")]
    RootIsNotDirectory(PathBuf),
}
