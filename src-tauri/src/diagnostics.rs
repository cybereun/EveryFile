use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rand::RngExt;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const APP_DATA_DIRECTORY: &str = "com.cybereun.everyfile";
const SECONDS_PER_DAY: u64 = 24 * 60 * 60;
const LOG_RETENTION_DAYS: u64 = 7;
const MAX_LOG_FILE_BYTES: u64 = 1024 * 1024;
const MAX_DAILY_LOG_SEGMENTS: u8 = 3;

pub struct DiagnosticsLogger {
    app_data_dir: PathBuf,
    log_path: PathBuf,
    redacted_roots: Vec<String>,
}

impl DiagnosticsLogger {
    pub fn new(
        app_data_dir: &Path,
        registered_roots: Vec<PathBuf>,
    ) -> Result<Self, DiagnosticError> {
        fs::create_dir_all(app_data_dir).map_err(DiagnosticError::Io)?;
        let app_data_dir = app_data_dir.canonicalize().map_err(DiagnosticError::Io)?;
        let logs_dir = app_data_dir.join("logs");
        fs::create_dir_all(&logs_dir).map_err(DiagnosticError::Io)?;
        let canonical_logs = logs_dir.canonicalize().map_err(DiagnosticError::Io)?;
        if !canonical_logs.starts_with(&app_data_dir) {
            return Err(DiagnosticError::UnsafeLogDirectory);
        }
        let log_path = canonical_logs.join(format!("diagnostics-{}.jsonl", current_day()));
        if fs::symlink_metadata(&log_path)
            .map(|metadata| metadata.file_type().is_symlink() || !metadata.is_file())
            .unwrap_or(false)
        {
            return Err(DiagnosticError::UnsafeLogDirectory);
        }
        let redacted_roots = registered_roots
            .into_iter()
            .flat_map(|root| {
                let display = root.to_string_lossy().into_owned();
                let slash = display.replace('\\', "/");
                let backslash = display.replace('/', "\\");
                [
                    display,
                    slash.clone(),
                    backslash.clone(),
                    format!(r"\\?\{backslash}"),
                    format!("//?/{slash}"),
                ]
            })
            .filter(|root| !root.is_empty())
            .collect::<Vec<_>>();
        let mut redacted_roots = redacted_roots;
        redacted_roots.sort_by_key(|root| std::cmp::Reverse(root.chars().count()));
        redacted_roots.dedup_by(|left, right| fold_windows_path(left) == fold_windows_path(right));
        let logger = Self {
            app_data_dir,
            log_path,
            redacted_roots,
        };
        logger.prune()?;
        Ok(logger)
    }

    pub fn write(&self, event: &DiagnosticEvent) -> Result<(), DiagnosticError> {
        validate_event(event)?;
        self.prune()?;
        let stored = StoredDiagnosticEvent {
            timestamp_unix_ms: now_unix_ms(),
            level: event.level.clone(),
            code: event.code.clone(),
            message: self.redact(&event.message),
            document_id: event.document_id.clone(),
        };
        let mut line = serde_json::to_vec(&stored).map_err(DiagnosticError::Serialize)?;
        line.push(b'\n');
        self.rotate_if_needed(u64::try_from(line.len()).unwrap_or(u64::MAX))?;
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.log_path)
            .map_err(DiagnosticError::Io)?;
        file.write_all(&line).map_err(DiagnosticError::Io)?;
        file.flush().map_err(DiagnosticError::Io)
    }

    pub fn current_log_path(&self) -> &Path {
        &self.log_path
    }

    pub fn log_directory(&self) -> PathBuf {
        self.app_data_dir.join("logs")
    }

    pub fn redact(&self, message: &str) -> String {
        let mut redacted = message.replace(['\r', '\n'], " ");
        for root in &self.redacted_roots {
            redacted = replace_path_case_insensitive(&redacted, root, "[REGISTERED_ROOT]");
        }
        redacted
    }

    pub fn prune(&self) -> Result<(), DiagnosticError> {
        let logs = self.app_data_dir.join("logs");
        let today = current_day();
        for entry in fs::read_dir(logs).map_err(DiagnosticError::Io)? {
            let entry = entry.map_err(DiagnosticError::Io)?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(DiagnosticError::Io)?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                continue;
            }
            let is_expired = log_day(&entry.file_name())
                .is_some_and(|day| today.saturating_sub(day) > LOG_RETENTION_DAYS);
            if is_expired {
                fs::remove_file(entry.path()).map_err(DiagnosticError::Io)?;
            }
        }
        Ok(())
    }

    fn rotate_if_needed(&self, incoming_bytes: u64) -> Result<(), DiagnosticError> {
        let current_size = fs::metadata(&self.log_path)
            .map(|metadata| metadata.len())
            .unwrap_or_default();
        if current_size.saturating_add(incoming_bytes) <= MAX_LOG_FILE_BYTES {
            return Ok(());
        }
        let base_name = self
            .log_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(DiagnosticError::UnsafeLogDirectory)?;
        for segment in (1..MAX_DAILY_LOG_SEGMENTS).rev() {
            let source = if segment == 1 {
                self.log_path.clone()
            } else {
                self.log_path
                    .with_file_name(format!("{base_name}.{}", segment - 1))
            };
            let destination = self
                .log_path
                .with_file_name(format!("{base_name}.{segment}"));
            if destination.exists() {
                fs::remove_file(&destination).map_err(DiagnosticError::Io)?;
            }
            if source.exists() {
                fs::rename(source, destination).map_err(DiagnosticError::Io)?;
            }
        }
        Ok(())
    }
}

fn validate_event(event: &DiagnosticEvent) -> Result<(), DiagnosticError> {
    if !matches!(event.level.as_str(), "debug" | "info" | "warn" | "error") {
        return Err(DiagnosticError::InvalidEvent(
            "unsupported diagnostic level".into(),
        ));
    }
    if event.code.is_empty()
        || event.code.len() > 80
        || !event.code.chars().all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
    {
        return Err(DiagnosticError::InvalidEvent(
            "diagnostic code is invalid".into(),
        ));
    }
    if event.message.len() > 16_384 {
        return Err(DiagnosticError::InvalidEvent(
            "diagnostic message is too long".into(),
        ));
    }
    Ok(())
}

fn replace_path_case_insensitive(haystack: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return haystack.to_owned();
    }
    let folded_needle = fold_windows_path(needle);
    let mut output = String::with_capacity(haystack.len());
    let mut byte_offset = 0;
    while byte_offset < haystack.len() {
        let remaining = &haystack[byte_offset..];
        let found = remaining.char_indices().find_map(|(relative, _)| {
            let start = byte_offset + relative;
            let mut end = start;
            let mut folded = String::new();
            for character in haystack[start..].chars() {
                if matches!(character, '\\' | '/') {
                    folded.push('\\');
                } else {
                    folded.extend(character.to_lowercase());
                }
                end += character.len_utf8();
                if folded.len() >= folded_needle.len() {
                    break;
                }
            }
            if folded != folded_needle {
                return None;
            }
            let follows_boundary = haystack[end..]
                .chars()
                .next()
                .is_none_or(|character| matches!(character, '\\' | '/'));
            follows_boundary.then_some((start, end))
        });
        let Some((found, end)) = found else {
            break;
        };
        output.push_str(&haystack[byte_offset..found]);
        output.push_str(replacement);
        byte_offset = end;
    }
    output.push_str(&haystack[byte_offset..]);
    output
}

fn fold_windows_path(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| {
            if matches!(character, '\\' | '/') {
                '\\'.to_lowercase()
            } else {
                character.to_lowercase()
            }
        })
        .collect()
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}

fn current_day() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() / SECONDS_PER_DAY)
        .unwrap_or_default()
}

fn log_day(file_name: &std::ffi::OsStr) -> Option<u64> {
    let name = file_name.to_str()?;
    let suffix = name.strip_prefix("diagnostics-")?;
    suffix.split('.').next()?.parse().ok()
}

pub fn validate_reset_target(
    local_appdata: &Path,
    target: &Path,
) -> Result<PathBuf, DiagnosticError> {
    if target
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(DiagnosticError::UnsafeResetTarget);
    }
    let local = local_appdata.canonicalize().map_err(DiagnosticError::Io)?;
    let expected_lexical = local_appdata.join(APP_DATA_DIRECTORY);
    if !paths_equal(target, &expected_lexical) {
        return Err(DiagnosticError::UnsafeResetTarget);
    }
    reject_reparse_point(local_appdata)?;
    reject_reparse_point(&expected_lexical)?;
    let expected = expected_lexical
        .canonicalize()
        .map_err(DiagnosticError::Io)?;
    let resolved = target.canonicalize().map_err(DiagnosticError::Io)?;
    if !resolved.starts_with(&local) || !paths_equal(&resolved, &expected) {
        return Err(DiagnosticError::UnsafeResetTarget);
    }
    Ok(resolved)
}

pub fn remove_app_data_contents(
    local_appdata: &Path,
    target: &Path,
) -> Result<(), DiagnosticError> {
    let target = validate_reset_target(local_appdata, target)?;
    #[cfg(windows)]
    {
        remove_windows_app_data_contents(local_appdata, &target, || {})
    }
    #[cfg(not(windows))]
    {
        let root_guard = ResetRootGuard::open(&target)?;
        for entry in fs::read_dir(&target).map_err(DiagnosticError::Io)? {
            let entry = entry.map_err(DiagnosticError::Io)?;
            quarantine_and_remove(&root_guard, &target, &entry.path())?;
        }
        Ok(())
    }
}

#[cfg(windows)]
fn remove_windows_app_data_contents(
    local_appdata: &Path,
    target: &Path,
    before_root_rename: impl FnOnce(),
) -> Result<(), DiagnosticError> {
    let local_guard = ResetRootGuard::open(local_appdata)?;
    let root = WindowsFileHandle::open_for_rename(target)?;
    if root.is_reparse() {
        return Err(DiagnosticError::UnsafeResetTarget);
    }
    let root_identity = root.identity;
    local_guard.revalidate()?;
    before_root_rename();
    let quarantine_name = (0..32)
        .find_map(|_| {
            let name = format!(
                ".{APP_DATA_DIRECTORY}-reset-quarantine-{:032x}",
                rand::rng().random::<u128>()
            );
            (!local_appdata.join(&name).exists()).then_some(name)
        })
        .ok_or(DiagnosticError::InvalidResetRequest)?;
    let quarantine = local_appdata.join(quarantine_name);
    root.rename_to(&quarantine)?;
    let pinned = WindowsFileHandle::open_pinned(&quarantine)?;
    if pinned.identity != root_identity || pinned.is_reparse() {
        return Err(DiagnosticError::UnsafeResetTarget);
    }
    fs::create_dir(target).map_err(DiagnosticError::Io)?;
    drop(root);
    remove_pinned_entry(&quarantine, pinned)
}

#[cfg(windows)]
pub fn start_reset_worker(app_data_dir: &Path) -> Result<(), DiagnosticError> {
    use std::os::windows::process::CommandExt;

    let local_appdata = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or(DiagnosticError::LocalAppDataUnavailable)?;
    let app_data_dir = validate_reset_target(&local_appdata, app_data_dir)?;
    let nonce = format!("{:032x}", rand::rng().random::<u128>());
    let parent_pid = std::process::id();
    let parent_created = current_process_creation_identity()?;
    let request_path = reset_request_path(&app_data_dir, &nonce)?;
    let mut request_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&request_path)
        .map_err(DiagnosticError::Io)?;
    writeln!(request_file, "{nonce}\n{parent_pid}\n{parent_created}")
        .map_err(DiagnosticError::Io)?;
    request_file.sync_all().map_err(DiagnosticError::Io)?;
    drop(request_file);
    let executable = std::env::current_exe().map_err(DiagnosticError::Io)?;
    let spawn = Command::new(executable)
        .arg("--everyfile-reset-after-exit")
        .arg(&nonce)
        .arg(parent_pid.to_string())
        .arg(parent_created.to_string())
        .creation_flags(0x0800_0000)
        .spawn();
    if let Err(error) = spawn {
        let _ = fs::remove_file(request_path);
        return Err(DiagnosticError::Io(error));
    }
    Ok(())
}

pub fn require_reset_confirmation(confirmed: bool) -> Result<(), DiagnosticError> {
    if confirmed {
        Ok(())
    } else {
        Err(DiagnosticError::ResetConfirmationRequired)
    }
}

#[cfg(not(windows))]
pub fn start_reset_worker(_app_data_dir: &Path) -> Result<(), DiagnosticError> {
    Err(DiagnosticError::ResetUnsupported)
}

pub fn run_reset_worker_from_args() -> bool {
    let arguments = std::env::args_os().collect::<Vec<_>>();
    if arguments.get(1).map(std::ffi::OsString::as_os_str)
        != Some(std::ffi::OsStr::new("--everyfile-reset-after-exit"))
    {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        if arguments.len() != 5 {
            return true;
        }
        let Some(nonce) = arguments[2]
            .to_str()
            .filter(|nonce| valid_reset_nonce(nonce))
        else {
            return true;
        };
        let Some(parent_pid) = arguments[3]
            .to_str()
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|value| *value != 0)
        else {
            return true;
        };
        let Some(parent_created) = arguments[4]
            .to_str()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value != 0)
        else {
            return true;
        };
        let Some(local_appdata) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) else {
            return true;
        };
        let target = local_appdata.join(APP_DATA_DIRECTORY);
        let Ok(target) = validate_reset_target(&local_appdata, &target) else {
            return true;
        };
        if wait_for_exact_process_exit(parent_pid, parent_created).is_err() {
            return true;
        }
        let Ok(request_path) = reset_request_path(&target, nonce) else {
            return true;
        };
        let Ok(metadata) = fs::symlink_metadata(&request_path) else {
            return true;
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 128 {
            return true;
        }
        let Ok(request) = fs::read_to_string(&request_path) else {
            return true;
        };
        if request != format!("{nonce}\n{parent_pid}\n{parent_created}\n") {
            return true;
        }
        if fs::remove_file(&request_path).is_err() {
            return true;
        }
        for _ in 0..300 {
            match remove_app_data_contents(&local_appdata, &target) {
                Ok(()) => {
                    if let Ok(executable) = std::env::current_exe() {
                        let _ = Command::new(executable).creation_flags(0x0800_0000).spawn();
                    }
                    return true;
                }
                Err(DiagnosticError::Io(_)) => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(_) => return true,
            }
        }
    }
    true
}

#[cfg(windows)]
fn current_process_creation_identity() -> Result<u64, DiagnosticError> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    }
    .map_err(windows_error)?;
    Ok(filetime_identity(created))
}

#[cfg(windows)]
fn wait_for_exact_process_exit(
    process_id: u32,
    expected_creation: u64,
) -> Result<(), DiagnosticError> {
    use windows::Win32::Foundation::{CloseHandle, FILETIME, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, WaitForSingleObject, PROCESS_ACCESS_RIGHTS,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    const SYNCHRONIZE_PROCESS: PROCESS_ACCESS_RIGHTS = PROCESS_ACCESS_RIGHTS(0x0010_0000);

    let process = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE_PROCESS,
            false,
            process_id,
        )
    }
    .map_err(windows_error)?;
    let result = (|| {
        let mut created = FILETIME::default();
        let mut exited = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        unsafe { GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user) }
            .map_err(windows_error)?;
        if filetime_identity(created) != expected_creation {
            return Err(DiagnosticError::InvalidResetRequest);
        }
        let wait = unsafe { WaitForSingleObject(process, u32::MAX) };
        if wait != WAIT_OBJECT_0 {
            return Err(DiagnosticError::InvalidResetRequest);
        }
        Ok(())
    })();
    unsafe {
        let _ = CloseHandle(process);
    }
    result
}

#[cfg(windows)]
fn filetime_identity(value: windows::Win32::Foundation::FILETIME) -> u64 {
    (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime)
}

#[cfg(windows)]
fn windows_error(error: windows::core::Error) -> DiagnosticError {
    DiagnosticError::Io(io::Error::other(error.to_string()))
}

fn valid_reset_nonce(nonce: &str) -> bool {
    nonce.len() == 32
        && nonce
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn reset_request_path(app_data_dir: &Path, nonce: &str) -> Result<PathBuf, DiagnosticError> {
    if !valid_reset_nonce(nonce) {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    Ok(app_data_dir.join(format!(".reset-request-{nonce}")))
}

#[cfg(not(windows))]
fn remove_entry_without_following(path: &Path) -> Result<(), DiagnosticError> {
    let metadata = fs::symlink_metadata(path).map_err(DiagnosticError::Io)?;
    if is_reparse_or_symlink(&metadata) {
        return remove_symlink(path, metadata.is_dir()).map_err(DiagnosticError::Io);
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path).map_err(DiagnosticError::Io)? {
            let entry = entry.map_err(DiagnosticError::Io)?;
            remove_entry_without_following(&entry.path())?;
        }
        fs::remove_dir(path).map_err(DiagnosticError::Io)
    } else if metadata.is_file() {
        fs::remove_file(path).map_err(DiagnosticError::Io)
    } else {
        Err(DiagnosticError::UnsafeResetTarget)
    }
}

#[cfg(not(windows))]
fn quarantine_and_remove(
    _parent_guard: &ResetRootGuard,
    parent: &Path,
    path: &Path,
) -> Result<(), DiagnosticError> {
    let file_name = path.file_name().ok_or(DiagnosticError::UnsafeResetTarget)?;
    let quarantine = (0..32)
        .find_map(|_| {
            let nonce = format!("{:032x}", rand::rng().random::<u128>());
            let candidate = parent.join(format!(".reset-quarantine-{nonce}"));
            (!candidate.exists()).then_some(candidate)
        })
        .ok_or(DiagnosticError::InvalidResetRequest)?;
    if path
        .parent()
        .is_none_or(|actual| !paths_equal(actual, parent))
        || file_name
            .to_string_lossy()
            .starts_with(".reset-quarantine-")
    {
        return Err(DiagnosticError::UnsafeResetTarget);
    }
    fs::rename(path, &quarantine).map_err(DiagnosticError::Io)?;
    remove_entry_without_following(&quarantine)
}

#[cfg(windows)]
fn quarantine_and_remove(
    parent_guard: &ResetRootGuard,
    parent: &Path,
    path: &Path,
) -> Result<(), DiagnosticError> {
    let file_name = path.file_name().ok_or(DiagnosticError::UnsafeResetTarget)?;
    if path
        .parent()
        .is_none_or(|actual| !paths_equal(actual, parent))
        || file_name
            .to_string_lossy()
            .starts_with(".reset-quarantine-")
    {
        return Err(DiagnosticError::UnsafeResetTarget);
    }
    let quarantine_name = (0..32)
        .find_map(|_| {
            let name = format!(".reset-quarantine-{:032x}", rand::rng().random::<u128>());
            (!parent.join(&name).exists()).then_some(name)
        })
        .ok_or(DiagnosticError::InvalidResetRequest)?;
    let child = WindowsFileHandle::open_for_rename(path)?;
    parent_guard.revalidate()?;
    let identity = child.identity;
    let quarantine = parent.join(quarantine_name);
    child.rename_to(&quarantine)?;
    let pinned = WindowsFileHandle::open_pinned(&quarantine)?;
    if pinned.identity != identity {
        return Err(DiagnosticError::UnsafeResetTarget);
    }
    drop(child);
    remove_pinned_entry(&quarantine, pinned)
}

#[cfg(windows)]
fn remove_pinned_entry(path: &Path, pinned: WindowsFileHandle) -> Result<(), DiagnosticError> {
    let metadata = fs::symlink_metadata(path).map_err(DiagnosticError::Io)?;
    if pinned.is_reparse() || is_reparse_or_symlink(&metadata) {
        drop(pinned);
        return remove_symlink(path, metadata.is_dir()).map_err(DiagnosticError::Io);
    }
    if metadata.is_dir() {
        let directory_guard = ResetRootGuard {
            handle: pinned.handle,
            identity: pinned.identity,
        };
        std::mem::forget(pinned);
        for entry in fs::read_dir(path).map_err(DiagnosticError::Io)? {
            let entry = entry.map_err(DiagnosticError::Io)?;
            quarantine_and_remove(&directory_guard, path, &entry.path())?;
        }
        drop(directory_guard);
        fs::remove_dir(path).map_err(DiagnosticError::Io)
    } else if metadata.is_file() {
        drop(pinned);
        fs::remove_file(path).map_err(DiagnosticError::Io)
    } else {
        Err(DiagnosticError::UnsafeResetTarget)
    }
}

fn reject_reparse_point(path: &Path) -> Result<(), DiagnosticError> {
    let metadata = fs::symlink_metadata(path).map_err(DiagnosticError::Io)?;
    if is_reparse_or_symlink(&metadata) || !metadata.is_dir() {
        Err(DiagnosticError::UnsafeResetTarget)
    } else {
        Ok(())
    }
}

fn is_reparse_or_symlink(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WindowsFileIdentity {
    volume: u32,
    index: u64,
}

#[cfg(windows)]
struct ResetRootGuard {
    handle: windows::Win32::Foundation::HANDLE,
    identity: WindowsFileIdentity,
}

#[cfg(windows)]
impl ResetRootGuard {
    fn open(path: &Path) -> Result<Self, DiagnosticError> {
        let file = WindowsFileHandle::open_pinned(path)?;
        if file.is_reparse() {
            return Err(DiagnosticError::UnsafeResetTarget);
        }
        let guard = Self {
            handle: file.handle,
            identity: file.identity,
        };
        std::mem::forget(file);
        Ok(guard)
    }

    fn revalidate(&self) -> Result<(), DiagnosticError> {
        let (identity, attributes) = file_information(self.handle)?;
        if identity != self.identity || attributes & 0x0000_0400 != 0 {
            Err(DiagnosticError::UnsafeResetTarget)
        } else {
            Ok(())
        }
    }
}

#[cfg(windows)]
impl Drop for ResetRootGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

#[cfg(windows)]
struct WindowsFileHandle {
    handle: windows::Win32::Foundation::HANDLE,
    identity: WindowsFileIdentity,
    attributes: u32,
}

#[cfg(windows)]
impl WindowsFileHandle {
    fn open_pinned(path: &Path) -> Result<Self, DiagnosticError> {
        Self::open(path, false)
    }

    fn open_for_rename(path: &Path) -> Result<Self, DiagnosticError> {
        Self::open(path, true)
    }

    fn open(path: &Path, share_delete: bool) -> Result<Self, DiagnosticError> {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::{
            CreateFileW, DELETE, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
            FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
            OPEN_EXISTING,
        };

        let wide = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let share = if share_delete {
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE
        } else {
            FILE_SHARE_READ | FILE_SHARE_WRITE
        };
        let access = if share_delete {
            FILE_READ_ATTRIBUTES | DELETE
        } else {
            FILE_READ_ATTRIBUTES
        };
        let handle = unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                access.0,
                share,
                None,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                None,
            )
        }
        .map_err(windows_error)?;
        let (identity, attributes) = match file_information(handle) {
            Ok(information) => information,
            Err(error) => {
                unsafe {
                    let _ = windows::Win32::Foundation::CloseHandle(handle);
                }
                return Err(error);
            }
        };
        Ok(Self {
            handle,
            identity,
            attributes,
        })
    }

    fn is_reparse(&self) -> bool {
        self.attributes & 0x0000_0400 != 0
    }

    fn rename_to(&self, destination: &Path) -> Result<(), DiagnosticError> {
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::Storage::FileSystem::{
            FileRenameInfo, SetFileInformationByHandle, FILE_RENAME_INFO,
        };

        let name = destination.as_os_str().encode_wide().collect::<Vec<_>>();
        let offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
        let byte_len = name
            .len()
            .checked_mul(std::mem::size_of::<u16>())
            .ok_or(DiagnosticError::InvalidResetRequest)?;
        let total = offset
            .checked_add(byte_len)
            .ok_or(DiagnosticError::InvalidResetRequest)?;
        let words = total.div_ceil(std::mem::size_of::<usize>());
        let mut buffer = vec![0_usize; words];
        let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        unsafe {
            (*info).Anonymous.ReplaceIfExists = false;
            (*info).RootDirectory = windows::Win32::Foundation::HANDLE::default();
            (*info).FileNameLength =
                u32::try_from(byte_len).map_err(|_| DiagnosticError::InvalidResetRequest)?;
            std::ptr::copy_nonoverlapping(
                name.as_ptr(),
                (info.cast::<u8>().add(offset)).cast::<u16>(),
                name.len(),
            );
            SetFileInformationByHandle(
                self.handle,
                FileRenameInfo,
                info.cast(),
                u32::try_from(total).map_err(|_| DiagnosticError::InvalidResetRequest)?,
            )
        }
        .map_err(windows_error)
    }
}

#[cfg(windows)]
fn file_information(
    handle: windows::Win32::Foundation::HANDLE,
) -> Result<(WindowsFileIdentity, u32), DiagnosticError> {
    use windows::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    unsafe { GetFileInformationByHandle(handle, &mut information) }.map_err(windows_error)?;
    Ok((
        WindowsFileIdentity {
            volume: information.dwVolumeSerialNumber,
            index: (u64::from(information.nFileIndexHigh) << 32)
                | u64::from(information.nFileIndexLow),
        },
        information.dwFileAttributes,
    ))
}

#[cfg(windows)]
impl Drop for WindowsFileHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

#[cfg(not(windows))]
struct ResetRootGuard;

#[cfg(not(windows))]
impl ResetRootGuard {
    fn open(path: &Path) -> Result<Self, DiagnosticError> {
        reject_reparse_point(path)?;
        Ok(Self)
    }
}

#[cfg(windows)]
fn remove_symlink(path: &Path, is_directory: bool) -> io::Result<()> {
    if is_directory {
        fs::remove_dir(path)
    } else {
        fs::remove_file(path).or_else(|_| fs::remove_dir(path))
    }
}

#[cfg(not(windows))]
fn remove_symlink(path: &Path, _is_directory: bool) -> io::Result<()> {
    fs::remove_file(path)
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticEvent {
    pub level: String,
    pub code: String,
    pub message: String,
    pub document_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredDiagnosticEvent {
    timestamp_unix_ms: u64,
    level: String,
    code: String,
    message: String,
    document_id: Option<String>,
}

#[derive(Debug, Error)]
pub enum DiagnosticError {
    #[error("diagnostics filesystem operation failed")]
    Io(#[source] io::Error),
    #[error("diagnostic event is invalid: {0}")]
    InvalidEvent(String),
    #[error("failed to serialize diagnostic event")]
    Serialize(#[source] serde_json::Error),
    #[error("diagnostic log directory is unsafe")]
    UnsafeLogDirectory,
    #[error("refusing to reset outside the EveryFile app-data directory")]
    UnsafeResetTarget,
    #[error("LOCALAPPDATA is unavailable")]
    LocalAppDataUnavailable,
    #[error("application reset is supported only on Windows")]
    ResetUnsupported,
    #[error("application data reset requires explicit confirmation")]
    ResetConfirmationRequired,
    #[error("application data reset request is invalid")]
    InvalidResetRequest,
}

impl DiagnosticError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "DIAGNOSTICS_IO_FAILED",
            Self::InvalidEvent(_) => "DIAGNOSTICS_EVENT_INVALID",
            Self::Serialize(_) => "DIAGNOSTICS_SERIALIZE_FAILED",
            Self::UnsafeLogDirectory => "DIAGNOSTICS_LOG_PATH_UNSAFE",
            Self::UnsafeResetTarget => "RESET_PATH_UNSAFE",
            Self::LocalAppDataUnavailable => "LOCAL_APP_DATA_UNAVAILABLE",
            Self::ResetUnsupported => "RESET_UNSUPPORTED",
            Self::ResetConfirmationRequired => "RESET_CONFIRMATION_REQUIRED",
            Self::InvalidResetRequest => "RESET_REQUEST_INVALID",
        }
    }
}

#[cfg(all(test, windows))]
mod windows_reset_tests {
    use super::*;
    use std::os::windows::process::CommandExt;

    #[test]
    fn by_handle_root_quarantine_detects_a_swap_and_leaves_the_victim_untouched() {
        let temp = tempfile::tempdir().unwrap();
        let local = temp.path().join("Local");
        let app_data = local.join(APP_DATA_DIRECTORY);
        let replacement = local.join("replacement");
        fs::create_dir_all(&app_data).unwrap();
        fs::create_dir_all(&replacement).unwrap();
        fs::write(app_data.join("owned"), b"owned").unwrap();
        fs::write(replacement.join("victim"), b"victim").unwrap();

        let moved_root = local.join("moved-root");
        let result = remove_windows_app_data_contents(&local, &app_data, || {
            fs::rename(&app_data, &moved_root).unwrap();
            fs::rename(&replacement, &app_data).unwrap();
        });

        assert!(
            result.is_err(),
            "a swapped target must abort reset: {result:?}"
        );
        assert_eq!(fs::read(app_data.join("victim")).unwrap(), b"victim");
        assert!(
            fs::read_dir(&local).unwrap().any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".com.cybereun.everyfile-reset-quarantine-")),
            "the owned root must be quarantined rather than a replacement being traversed: {result:?}"
        );
    }

    #[test]
    fn reset_waits_for_the_exact_process_creation_identity() {
        use windows::Win32::Foundation::{CloseHandle, FILETIME};
        use windows::Win32::System::Threading::{
            GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };

        let mut child = Command::new("ping")
            .args(["127.0.0.1", "-n", "2"])
            .creation_flags(0x0800_0000)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let handle =
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, child.id()) }.unwrap();
        let mut created = FILETIME::default();
        let mut exited = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) }
            .unwrap();
        unsafe {
            CloseHandle(handle).unwrap();
        }
        let identity = filetime_identity(created);

        assert!(wait_for_exact_process_exit(child.id(), identity.wrapping_add(1)).is_err());
        wait_for_exact_process_exit(child.id(), identity).unwrap();
        child.wait().unwrap();
    }
}
