use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use rand::RngExt;
use rust_xlsxwriter::Workbook;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::models::{SearchHit, SearchMatchKind};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    Csv,
    Xlsx,
    Markdown,
}

impl ExportFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Xlsx => "xlsx",
            Self::Markdown => "md",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ExportRequest {
    SearchResults { hits: Vec<SearchHit> },
    MarkdownDocument { file_name: String, markdown: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ExportOutcome {
    Written,
    Cancelled,
}

pub fn export_to_destination(
    request: &ExportRequest,
    format: ExportFormat,
    destination: Option<&Path>,
) -> Result<ExportOutcome, ExportError> {
    let Some(destination) = destination else {
        return Ok(ExportOutcome::Cancelled);
    };
    validate_request_format(request, format)?;
    let destination = validate_destination(destination, format)?;
    let bytes = render(request, format)?;
    atomic_write(&destination, &bytes)?;
    Ok(ExportOutcome::Written)
}

fn validate_request_format(
    request: &ExportRequest,
    format: ExportFormat,
) -> Result<(), ExportError> {
    match (request, format) {
        (ExportRequest::SearchResults { .. }, ExportFormat::Csv | ExportFormat::Xlsx)
        | (ExportRequest::MarkdownDocument { .. }, ExportFormat::Markdown) => Ok(()),
        _ => Err(ExportError::RequestFormatMismatch),
    }
}

fn validate_destination(destination: &Path, format: ExportFormat) -> Result<PathBuf, ExportError> {
    if !destination.is_absolute() {
        return Err(ExportError::InvalidDestination(
            "destination must be absolute".into(),
        ));
    }
    if destination
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(ExportError::InvalidDestination(
            "destination cannot contain parent traversal".into(),
        ));
    }
    let file_name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ExportError::InvalidDestination("file name is invalid".into()))?;
    if file_name.chars().any(char::is_control) {
        return Err(ExportError::InvalidDestination(
            "file name contains control characters".into(),
        ));
    }
    let extension = destination
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !extension.eq_ignore_ascii_case(format.extension()) {
        return Err(ExportError::InvalidExtension {
            expected: format.extension(),
        });
    }
    let parent = destination
        .parent()
        .ok_or_else(|| ExportError::InvalidDestination("destination has no parent".into()))?;
    let parent_metadata = fs::metadata(parent).map_err(ExportError::DestinationUnavailable)?;
    if !parent_metadata.is_dir() {
        return Err(ExportError::InvalidDestination(
            "destination parent is not a directory".into(),
        ));
    }
    if let Ok(metadata) = fs::symlink_metadata(destination) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(ExportError::InvalidDestination(
                "destination must be a regular file".into(),
            ));
        }
    }
    Ok(destination.to_path_buf())
}

fn render(request: &ExportRequest, format: ExportFormat) -> Result<Vec<u8>, ExportError> {
    match (request, format) {
        (ExportRequest::SearchResults { hits }, ExportFormat::Csv) => Ok(render_csv(hits)),
        (ExportRequest::SearchResults { hits }, ExportFormat::Xlsx) => render_xlsx(hits),
        (ExportRequest::MarkdownDocument { markdown, .. }, ExportFormat::Markdown) => {
            Ok(markdown.as_bytes().to_vec())
        }
        _ => Err(ExportError::RequestFormatMismatch),
    }
}

fn render_csv(hits: &[SearchHit]) -> Vec<u8> {
    let mut output = String::from(
        "\u{feff}fileName,path,extension,sizeBytes,modifiedAt,excerpt,score,matchKind\r\n",
    );
    for hit in hits {
        let fields = [
            csv_text(&hit.file_name),
            csv_text(&hit.path),
            csv_text(&hit.extension),
            hit.size_bytes.to_string(),
            csv_text(&hit.modified_at),
            csv_text(hit.snippet.as_deref().unwrap_or_default()),
            hit.score.to_string(),
            match_kind_name(hit.match_kind).to_owned(),
        ];
        output.push_str(
            &fields
                .iter()
                .map(|field| csv_quote(field))
                .collect::<Vec<_>>()
                .join(","),
        );
        output.push_str("\r\n");
    }
    output.into_bytes()
}

fn csv_text(value: &str) -> String {
    let first_significant = value
        .chars()
        .find(|character| !character.is_whitespace() && !character.is_control());
    if matches!(first_significant, Some('=' | '+' | '-' | '@')) {
        format!("'{value}")
    } else {
        value.to_owned()
    }
}

fn csv_quote(value: &str) -> String {
    if value.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn render_xlsx(hits: &[SearchHit]) -> Result<Vec<u8>, ExportError> {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet.set_name("Search Results")?;
    for (column, header) in [
        "fileName",
        "path",
        "extension",
        "sizeBytes",
        "modifiedAt",
        "excerpt",
        "score",
        "matchKind",
    ]
    .into_iter()
    .enumerate()
    {
        worksheet.write_string(0, u16::try_from(column).unwrap_or_default(), header)?;
    }
    for (index, hit) in hits.iter().enumerate() {
        let row = u32::try_from(index + 1).map_err(|_| ExportError::TooManyRows)?;
        worksheet.write_string(row, 0, &hit.file_name)?;
        worksheet.write_string(row, 1, &hit.path)?;
        worksheet.write_string(row, 2, &hit.extension)?;
        worksheet.write_number(row, 3, hit.size_bytes as f64)?;
        worksheet.write_string(row, 4, &hit.modified_at)?;
        worksheet.write_string(row, 5, hit.snippet.as_deref().unwrap_or_default())?;
        worksheet.write_number(row, 6, hit.score)?;
        worksheet.write_string(row, 7, match_kind_name(hit.match_kind))?;
    }
    workbook.save_to_buffer().map_err(ExportError::Workbook)
}

fn match_kind_name(kind: SearchMatchKind) -> &'static str {
    match kind {
        SearchMatchKind::Filename => "filename",
        SearchMatchKind::Content => "content",
        SearchMatchKind::Both => "both",
        SearchMatchKind::Metadata => "metadata",
    }
}

fn atomic_write(destination: &Path, bytes: &[u8]) -> Result<(), ExportError> {
    let parent = destination
        .parent()
        .ok_or_else(|| ExportError::InvalidDestination("destination has no parent".into()))?;
    let file_name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| ExportError::InvalidDestination("file name is invalid".into()))?;
    let mut created = None;
    for _ in 0..32 {
        let suffix = rand::rng().random::<u64>();
        let candidate = parent.join(format!(".{file_name}.{suffix:016x}.tmp"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                created = Some((candidate, file));
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(ExportError::Write(error)),
        }
    }
    let (temporary, mut file) = created.ok_or(ExportError::TemporaryNameExhausted)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        atomic_replace(&temporary, destination)?;
        if let Ok(directory) = OpenOptions::new().read(true).open(parent) {
            let _ = directory.sync_all();
        }
        Ok::<(), io::Error>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(ExportError::Write)
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(destination.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
        .map_err(io::Error::other)
    }
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("export request does not match the selected format")]
    RequestFormatMismatch,
    #[error("invalid export destination: {0}")]
    InvalidDestination(String),
    #[error("export destination must use .{expected}")]
    InvalidExtension { expected: &'static str },
    #[error("export destination is unavailable")]
    DestinationUnavailable(#[source] io::Error),
    #[error("failed to render Excel workbook")]
    Workbook(#[from] rust_xlsxwriter::XlsxError),
    #[error("export has too many rows")]
    TooManyRows,
    #[error("could not allocate a temporary export file")]
    TemporaryNameExhausted,
    #[error("failed to write export")]
    Write(#[source] io::Error),
}

impl ExportError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::RequestFormatMismatch => "EXPORT_FORMAT_MISMATCH",
            Self::InvalidDestination(_) | Self::InvalidExtension { .. } => {
                "EXPORT_DESTINATION_INVALID"
            }
            Self::DestinationUnavailable(_) => "EXPORT_DESTINATION_UNAVAILABLE",
            Self::Workbook(_) => "EXPORT_RENDER_FAILED",
            Self::TooManyRows => "EXPORT_TOO_LARGE",
            Self::TemporaryNameExhausted | Self::Write(_) => "EXPORT_WRITE_FAILED",
        }
    }
}
