use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use rusqlite::OptionalExtension;
use thiserror::Error;

use crate::infrastructure::database::Database;

pub struct VerifiedSource {
    canonical_path: PathBuf,
    #[cfg(windows)]
    _locks: Vec<WindowsPathLock>,
}

impl VerifiedSource {
    pub fn path(&self) -> &Path {
        &self.canonical_path
    }

    pub fn current_path(&self) -> Result<PathBuf, SourceOpenError> {
        #[cfg(windows)]
        {
            for lock in &self._locks {
                lock.ensure_non_redirecting()?;
            }
            let root = self
                ._locks
                .first()
                .ok_or_else(|| SourceOpenError::Platform("missing root identity handle".into()))?
                .current_path()?;
            let source = self
                ._locks
                .last()
                .ok_or_else(|| SourceOpenError::Platform("missing source identity handle".into()))?
                .current_path()?;
            if !source.is_file() {
                return Err(SourceOpenError::NotFile);
            }
            if !source.starts_with(&root) {
                return Err(SourceOpenError::OutsideRegisteredRoot);
            }
            Ok(source)
        }
        #[cfg(not(windows))]
        {
            Ok(self.canonical_path.clone())
        }
    }
}

pub fn verify_indexed_source(
    database: &Database,
    document_id: &str,
) -> Result<VerifiedSource, SourceOpenError> {
    if document_id.trim().is_empty() {
        return Err(SourceOpenError::NotFound);
    }
    let record = database
        .connection()
        .query_row(
            "SELECT d.canonical_path, f.canonical_path, f.enabled
             FROM documents d
             JOIN folders f ON f.id = d.folder_id
             WHERE d.id = ?1",
            [document_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, bool>(2)?,
                ))
            },
        )
        .optional()?
        .ok_or(SourceOpenError::NotFound)?;
    if !record.2 {
        return Err(SourceOpenError::DisabledFolder);
    }

    let source_path = PathBuf::from(record.0);
    let root_path = PathBuf::from(record.1);
    if !source_path.starts_with(&root_path) {
        return Err(SourceOpenError::OutsideRegisteredRoot);
    }

    #[cfg(windows)]
    let locks = lock_windows_path_components(&root_path, &source_path)?;

    let root = root_path
        .canonicalize()
        .map_err(SourceOpenError::Unavailable)?;
    let source = source_path
        .canonicalize()
        .map_err(SourceOpenError::Unavailable)?;
    if !source.is_file() {
        return Err(SourceOpenError::NotFile);
    }
    if !source.starts_with(&root) {
        return Err(SourceOpenError::OutsideRegisteredRoot);
    }

    Ok(VerifiedSource {
        canonical_path: source,
        #[cfg(windows)]
        _locks: locks,
    })
}

pub fn resolve_indexed_source(
    database: &Database,
    document_id: &str,
) -> Result<PathBuf, SourceOpenError> {
    verify_indexed_source(database, document_id)?.current_path()
}

pub fn open_indexed_source(database: &Database, document_id: &str) -> Result<(), SourceOpenError> {
    let verified = verify_indexed_source(database, document_id)?;
    let source = verified.current_path()?;
    launch_source(&source)
}

pub fn open_indexed_location(
    database: &Database,
    document_id: &str,
) -> Result<(), SourceOpenError> {
    let verified = verify_indexed_source(database, document_id)?;
    let source = verified.current_path()?;
    launch_location(&source)
}

pub fn open_registered_folder(database: &Database, folder_id: &str) -> Result<(), SourceOpenError> {
    let path: String = database
        .connection()
        .query_row(
            "SELECT canonical_path FROM folders WHERE id = ?1 AND enabled = 1",
            [folder_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(SourceOpenError::NotFound)?;
    let path = PathBuf::from(path)
        .canonicalize()
        .map_err(SourceOpenError::Unavailable)?;
    if !path.is_dir() {
        return Err(SourceOpenError::NotFile);
    }
    launch_source(&path)
}

pub fn read_indexed_pdf(
    database: &Database,
    document_id: &str,
) -> Result<Vec<u8>, SourceOpenError> {
    read_indexed_pdf_cancellable(database, document_id, || false)
}

pub fn read_indexed_pdf_cancellable(
    database: &Database,
    document_id: &str,
    cancelled: impl Fn() -> bool,
) -> Result<Vec<u8>, SourceOpenError> {
    const MAX_PDF_BYTES: u64 = 128 * 1024 * 1024;
    let verified = verify_indexed_source(database, document_id)?;
    let source = verified.current_path()?;
    if source
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| !extension.eq_ignore_ascii_case("pdf"))
        .unwrap_or(true)
    {
        return Err(SourceOpenError::NotPdf);
    }
    let metadata = std::fs::metadata(&source).map_err(SourceOpenError::Unavailable)?;
    if metadata.len() > MAX_PDF_BYTES {
        return Err(SourceOpenError::TooLarge);
    }
    if cancelled() {
        return Err(SourceOpenError::Cancelled);
    }
    let mut file = std::fs::File::open(&source).map_err(SourceOpenError::Unavailable)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    let mut chunk = vec![0_u8; 1024 * 1024];
    loop {
        if cancelled() {
            return Err(SourceOpenError::Cancelled);
        }
        let read = file
            .read(&mut chunk)
            .map_err(SourceOpenError::Unavailable)?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    if cancelled() {
        return Err(SourceOpenError::Cancelled);
    }
    if !bytes.starts_with(b"%PDF-") {
        return Err(SourceOpenError::NotPdf);
    }
    Ok(bytes)
}

pub fn read_indexed_layout_cancellable(
    database: &Database,
    document_id: &str,
    cancelled: impl Fn() -> bool,
) -> Result<Vec<u8>, SourceOpenError> {
    const MAX_LAYOUT_BYTES: u64 = 128 * 1024 * 1024;
    let verified = verify_indexed_source(database, document_id)?;
    let source = verified.current_path()?;
    let extension = source
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "hwp" | "hwpx" | "pdf") {
        return Err(SourceOpenError::NotLayoutPreview);
    }
    let metadata = std::fs::metadata(&source).map_err(SourceOpenError::Unavailable)?;
    if metadata.len() > MAX_LAYOUT_BYTES {
        return Err(SourceOpenError::TooLarge);
    }
    if cancelled() {
        return Err(SourceOpenError::Cancelled);
    }
    let mut file = std::fs::File::open(&source).map_err(SourceOpenError::Unavailable)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    let mut chunk = vec![0_u8; 1024 * 1024];
    loop {
        if cancelled() {
            return Err(SourceOpenError::Cancelled);
        }
        let read = file
            .read(&mut chunk)
            .map_err(SourceOpenError::Unavailable)?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    let valid_magic = match extension.as_str() {
        "pdf" => bytes.starts_with(b"%PDF-"),
        "hwp" => bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]),
        "hwpx" => bytes.starts_with(b"PK"),
        _ => false,
    };
    if !valid_magic {
        return Err(SourceOpenError::NotLayoutPreview);
    }
    Ok(bytes)
}

#[cfg(windows)]
struct WindowsPathLock(windows::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl WindowsPathLock {
    fn ensure_non_redirecting(&self) -> Result<(), SourceOpenError> {
        use windows::Win32::Storage::FileSystem::{
            FileAttributeTagInfo, GetFileInformationByHandleEx, FILE_ATTRIBUTE_REPARSE_POINT,
            FILE_ATTRIBUTE_TAG_INFO,
        };

        let mut tag = FILE_ATTRIBUTE_TAG_INFO::default();
        unsafe {
            GetFileInformationByHandleEx(
                self.0,
                FileAttributeTagInfo,
                (&mut tag as *mut FILE_ATTRIBUTE_TAG_INFO).cast(),
                std::mem::size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
            )
        }
        .map_err(|error| SourceOpenError::Platform(error.to_string()))?;
        if is_redirecting_reparse_tag(
            tag.FileAttributes,
            FILE_ATTRIBUTE_REPARSE_POINT.0,
            tag.ReparseTag,
        ) {
            return Err(SourceOpenError::RedirectingReparsePoint);
        }
        Ok(())
    }

    fn current_path(&self) -> Result<PathBuf, SourceOpenError> {
        use std::os::windows::ffi::OsStringExt;
        use windows::Win32::Storage::FileSystem::{
            GetFinalPathNameByHandleW, FILE_NAME_NORMALIZED,
        };

        let mut buffer = vec![0_u16; 512];
        loop {
            let length =
                unsafe { GetFinalPathNameByHandleW(self.0, &mut buffer, FILE_NAME_NORMALIZED) }
                    as usize;
            if length == 0 {
                return Err(SourceOpenError::Platform(
                    std::io::Error::last_os_error().to_string(),
                ));
            }
            if length < buffer.len() {
                return Ok(PathBuf::from(std::ffi::OsString::from_wide(
                    &buffer[..length],
                )));
            }
            buffer.resize(length + 1, 0);
        }
    }
}

#[cfg(windows)]
impl Drop for WindowsPathLock {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
fn lock_windows_path_components(
    root: &Path,
    source: &Path,
) -> Result<Vec<WindowsPathLock>, SourceOpenError> {
    let relative = source
        .strip_prefix(root)
        .map_err(|_| SourceOpenError::OutsideRegisteredRoot)?;
    let mut paths = vec![root.to_path_buf()];
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        paths.push(current.clone());
    }

    paths.into_iter().map(lock_windows_component).collect()
}

#[cfg(windows)]
fn lock_windows_component(path: PathBuf) -> Result<WindowsPathLock, SourceOpenError> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_SHARE_READ, OPEN_EXISTING,
    };

    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_READ_DATA.0 | FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            None,
        )
    }
    .map_err(|error| {
        let io_error = std::io::Error::last_os_error();
        if io_error.kind() == std::io::ErrorKind::NotFound {
            SourceOpenError::Unavailable(io_error)
        } else {
            SourceOpenError::Platform(error.to_string())
        }
    })?;
    let lock = WindowsPathLock(handle);
    lock.ensure_non_redirecting()?;
    Ok(lock)
}

#[cfg(windows)]
fn is_redirecting_reparse_tag(attributes: u32, reparse_attribute: u32, tag: u32) -> bool {
    const IO_REPARSE_TAG_NAME_SURROGATE: u32 = 0x2000_0000;
    attributes & reparse_attribute != 0 && tag & IO_REPARSE_TAG_NAME_SURROGATE != 0
}

#[cfg(windows)]
fn shell_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        value.strip_prefix(r"\\?\").unwrap_or(&value).to_owned()
    }
}

#[cfg(windows)]
fn launch_source(source: &Path) -> Result<(), SourceOpenError> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_UNICODE, SHELLEXECUTEINFOW,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let verb = "open\0".encode_utf16().collect::<Vec<_>>();
    let file = std::ffi::OsStr::new(&shell_path(source))
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC | SEE_MASK_UNICODE,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // The write-exclusive verification handles remain alive through this synchronous
    // shell handoff and are released immediately after ShellExecuteExW returns.
    unsafe { ShellExecuteExW(&mut info) }
        .map_err(|error| SourceOpenError::Launch(error.to_string()))
}

#[cfg(windows)]
fn launch_location(source: &Path) -> Result<(), SourceOpenError> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    Command::new("explorer.exe")
        .arg(format!("/select,{}", shell_path(source)))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|error| SourceOpenError::Launch(error.to_string()))
}

#[cfg(target_os = "macos")]
fn launch_source(source: &Path) -> Result<(), SourceOpenError> {
    Command::new("open")
        .arg(source)
        .spawn()
        .map(|_| ())
        .map_err(|error| SourceOpenError::Launch(error.to_string()))
}

#[cfg(target_os = "macos")]
fn launch_location(source: &Path) -> Result<(), SourceOpenError> {
    Command::new("open")
        .arg("-R")
        .arg(source)
        .spawn()
        .map(|_| ())
        .map_err(|error| SourceOpenError::Launch(error.to_string()))
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn launch_source(source: &Path) -> Result<(), SourceOpenError> {
    Command::new("xdg-open")
        .arg(source)
        .spawn()
        .map(|_| ())
        .map_err(|error| SourceOpenError::Launch(error.to_string()))
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn launch_location(source: &Path) -> Result<(), SourceOpenError> {
    let parent = source.parent().ok_or(SourceOpenError::NotFile)?;
    Command::new("xdg-open")
        .arg(parent)
        .spawn()
        .map(|_| ())
        .map_err(|error| SourceOpenError::Launch(error.to_string()))
}

#[derive(Debug, Error)]
pub enum SourceOpenError {
    #[error("indexed document was not found")]
    NotFound,
    #[error("indexed document belongs to a disabled folder")]
    DisabledFolder,
    #[error("indexed source is unavailable")]
    Unavailable(#[source] std::io::Error),
    #[error("indexed source is not a file")]
    NotFile,
    #[error("indexed source is not a PDF")]
    NotPdf,
    #[error("indexed source does not support original-layout preview")]
    NotLayoutPreview,
    #[error("indexed PDF exceeds the preview size limit")]
    TooLarge,
    #[error("indexed PDF read was cancelled")]
    Cancelled,
    #[error("indexed source resolved outside its registered folder")]
    OutsideRegisteredRoot,
    #[error("redirecting filesystem reparse points are not allowed")]
    RedirectingReparsePoint,
    #[error("source verification failed: {0}")]
    Platform(String),
    #[error("failed to open indexed source: {0}")]
    Launch(String),
    #[error("source lookup failed")]
    Database(#[from] rusqlite::Error),
}

impl SourceOpenError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "SOURCE_NOT_FOUND",
            Self::DisabledFolder => "SOURCE_FOLDER_DISABLED",
            Self::Unavailable(_) => "SOURCE_UNAVAILABLE",
            Self::NotFile => "SOURCE_NOT_FILE",
            Self::NotPdf => "SOURCE_NOT_PDF",
            Self::NotLayoutPreview => "SOURCE_LAYOUT_UNSUPPORTED",
            Self::TooLarge => "SOURCE_PDF_TOO_LARGE",
            Self::Cancelled => "SOURCE_PDF_READ_CANCELLED",
            Self::OutsideRegisteredRoot => "SOURCE_OUTSIDE_REGISTERED_ROOT",
            Self::RedirectingReparsePoint => "SOURCE_REDIRECTING_REPARSE_POINT",
            Self::Platform(_) => "SOURCE_VERIFICATION_FAILED",
            Self::Launch(_) => "SOURCE_OPEN_FAILED",
            Self::Database(_) => "SOURCE_LOOKUP_FAILED",
        }
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::{is_redirecting_reparse_tag, shell_path};
    use std::path::Path;

    #[test]
    fn shell_path_removes_extended_prefixes_without_losing_unicode_or_unc() {
        assert_eq!(
            shell_path(Path::new(r"\\?\C:\My Files\보고서, 최종.pdf")),
            r"C:\My Files\보고서, 최종.pdf"
        );
        assert_eq!(
            shell_path(Path::new(r"\\?\UNC\server\share\보고서.pdf")),
            r"\\server\share\보고서.pdf"
        );
    }

    #[test]
    fn reparse_policy_rejects_only_name_surrogates() {
        const REPARSE_ATTRIBUTE: u32 = 0x400;
        const SYMLINK: u32 = 0xA000_000C;
        const MOUNT_POINT: u32 = 0xA000_0003;
        const CLOUD: u32 = 0x9000_001A;

        assert!(is_redirecting_reparse_tag(
            REPARSE_ATTRIBUTE,
            REPARSE_ATTRIBUTE,
            SYMLINK
        ));
        assert!(is_redirecting_reparse_tag(
            REPARSE_ATTRIBUTE,
            REPARSE_ATTRIBUTE,
            MOUNT_POINT
        ));
        assert!(!is_redirecting_reparse_tag(
            REPARSE_ATTRIBUTE,
            REPARSE_ATTRIBUTE,
            CLOUD
        ));
        assert!(!is_redirecting_reparse_tag(0, REPARSE_ATTRIBUTE, SYMLINK));
    }
}
