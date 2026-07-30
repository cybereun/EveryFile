use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
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
#[cfg(windows)]
const MAX_RESET_STATE_BYTES: u64 = 1024;

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
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct ResetRequest {
    nonce: String,
    parent_pid: u32,
    parent_created: u64,
}

#[cfg(windows)]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct ResetOwnershipRecord {
    nonce: String,
    root_identity: WindowsFileIdentity,
    quarantine_name: String,
}

#[cfg(windows)]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct ResetStageRecord {
    nonce: String,
    staging_name: String,
    staging_identity: WindowsFileIdentity,
}

#[cfg(windows)]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct ResetCommitRecord {
    request: ResetRequest,
    staging_name: String,
    target_identity: WindowsFileIdentity,
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResetCleanupStep {
    Deleting,
    Deleted,
    Staged,
    Owner,
    ParentExited,
    Request,
    Committed,
}

#[cfg(windows)]
#[cfg_attr(not(test), allow(dead_code))]
enum ResetEvent {
    BeforeQuarantineCleanup { attempt: usize },
    AfterQuarantineDeletion,
    AfterTargetRename,
    BeforeStateCleanup(ResetCleanupStep),
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResetWorkerOutcome {
    Success,
    TargetConflict,
    RetryExhausted,
    Failed,
}

#[cfg(windows)]
pub fn start_reset_worker(app_data_dir: &Path) -> Result<(), DiagnosticError> {
    use std::os::windows::process::CommandExt;

    let local_appdata = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or(DiagnosticError::LocalAppDataUnavailable)?;
    validate_reset_target(&local_appdata, app_data_dir)?;
    let nonce = format!("{:032x}", rand::rng().random::<u128>());
    let parent_pid = std::process::id();
    let parent_created = current_process_creation_identity()?;
    let local_guard = ResetRootGuard::open(&local_appdata)?;
    let request_path = reset_request_path(&local_appdata, &nonce)?;
    create_reset_state(
        &local_guard,
        &local_appdata,
        &request_path,
        &ResetRequest {
            nonce: nonce.clone(),
            parent_pid,
            parent_created,
        },
    )?;
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
        let Ok(request_path) = reset_request_path(&local_appdata, nonce) else {
            return true;
        };
        let Ok(commit_path) = reset_commit_path(&local_appdata, nonce) else {
            return true;
        };
        let Ok(parent_exited_path) = reset_parent_exited_path(&local_appdata, nonce) else {
            return true;
        };
        let Ok(local_guard) = ResetRootGuard::open(&local_appdata) else {
            return true;
        };
        let Ok(request) = read_authenticated_reset_request(
            &local_guard,
            &local_appdata,
            &request_path,
            &commit_path,
            nonce,
        ) else {
            return true;
        };
        if request
            != (ResetRequest {
                nonce: nonce.to_owned(),
                parent_pid,
                parent_created,
            })
        {
            return true;
        }
        if parent_exited_path.exists() {
            let Ok(recorded) =
                read_reset_state::<ResetRequest>(&local_guard, &local_appdata, &parent_exited_path)
            else {
                return true;
            };
            if recorded != request {
                return true;
            }
        } else {
            if wait_for_exact_process_exit(parent_pid, parent_created).is_err() {
                return true;
            }
            if create_reset_state(&local_guard, &local_appdata, &parent_exited_path, &request)
                .is_err()
            {
                return true;
            }
        }
        if run_owned_reset_transaction(
            &local_appdata,
            &target,
            nonce,
            300,
            Duration::from_millis(100),
            |_| Ok(()),
        ) == ResetWorkerOutcome::Success
        {
            if let Ok(executable) = std::env::current_exe() {
                let _ = Command::new(executable).creation_flags(0x0800_0000).spawn();
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

    let process = match unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE_PROCESS,
            false,
            process_id,
        )
    } {
        Ok(process) => process,
        Err(error) => {
            let error = windows_error(error);
            if matches!(
                &error,
                DiagnosticError::Io(error) if error.raw_os_error() == Some(87)
            ) {
                return Ok(());
            }
            return Err(error);
        }
    };
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
    let code = error.code().0 as u32;
    let os_code = if code & 0xFFFF_0000 == 0x8007_0000 {
        code & 0x0000_FFFF
    } else {
        code
    };
    DiagnosticError::Io(io::Error::from_raw_os_error(os_code as i32))
}

fn valid_reset_nonce(nonce: &str) -> bool {
    nonce.len() == 32
        && nonce
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn reset_request_path(local_appdata: &Path, nonce: &str) -> Result<PathBuf, DiagnosticError> {
    if !valid_reset_nonce(nonce) {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    Ok(local_appdata.join(format!(".{APP_DATA_DIRECTORY}-reset-request-{nonce}.json")))
}

#[cfg(windows)]
fn reset_owner_path(local_appdata: &Path, nonce: &str) -> Result<PathBuf, DiagnosticError> {
    if !valid_reset_nonce(nonce) {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    Ok(local_appdata.join(format!(".{APP_DATA_DIRECTORY}-reset-owner-{nonce}.json")))
}

#[cfg(windows)]
fn reset_cleaned_path(local_appdata: &Path, nonce: &str) -> Result<PathBuf, DiagnosticError> {
    if !valid_reset_nonce(nonce) {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    Ok(local_appdata.join(format!(".{APP_DATA_DIRECTORY}-reset-cleaned-{nonce}.json")))
}

#[cfg(windows)]
fn reset_deleting_path(local_appdata: &Path, nonce: &str) -> Result<PathBuf, DiagnosticError> {
    if !valid_reset_nonce(nonce) {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    Ok(local_appdata.join(format!(".{APP_DATA_DIRECTORY}-reset-deleting-{nonce}.json")))
}

#[cfg(windows)]
fn reset_stage_path(local_appdata: &Path, nonce: &str) -> Result<PathBuf, DiagnosticError> {
    if !valid_reset_nonce(nonce) {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    Ok(local_appdata.join(format!(".{APP_DATA_DIRECTORY}-reset-staged-{nonce}.json")))
}

#[cfg(windows)]
fn reset_parent_exited_path(local_appdata: &Path, nonce: &str) -> Result<PathBuf, DiagnosticError> {
    if !valid_reset_nonce(nonce) {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    Ok(local_appdata.join(format!(
        ".{APP_DATA_DIRECTORY}-reset-parent-exited-{nonce}.json"
    )))
}

#[cfg(windows)]
fn reset_commit_path(local_appdata: &Path, nonce: &str) -> Result<PathBuf, DiagnosticError> {
    if !valid_reset_nonce(nonce) {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    Ok(local_appdata.join(format!(
        ".{APP_DATA_DIRECTORY}-reset-committed-{nonce}.json"
    )))
}

#[cfg(windows)]
fn reset_quarantine_name(nonce: &str) -> Result<String, DiagnosticError> {
    if !valid_reset_nonce(nonce) {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    Ok(format!(".{APP_DATA_DIRECTORY}-reset-quarantine-{nonce}"))
}

#[cfg(windows)]
fn reset_staging_name(nonce: &str) -> Result<String, DiagnosticError> {
    if !valid_reset_nonce(nonce) {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    Ok(format!(".{APP_DATA_DIRECTORY}-reset-empty-{nonce}"))
}

#[cfg(windows)]
fn create_reset_state<T: Serialize>(
    local_guard: &ResetRootGuard,
    local_appdata: &Path,
    path: &Path,
    state: &T,
) -> Result<(), DiagnosticError> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;

    if path
        .parent()
        .is_none_or(|parent| !paths_equal(parent, local_appdata))
    {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    let bytes = serde_json::to_vec(state).map_err(DiagnosticError::Serialize)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_RESET_STATE_BYTES {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    local_guard.revalidate()?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)
        .map_err(DiagnosticError::Io)?;
    file.write_all(&bytes).map_err(DiagnosticError::Io)?;
    file.sync_all().map_err(DiagnosticError::Io)?;
    local_guard.revalidate()
}

#[cfg(windows)]
fn read_reset_state<T: serde::de::DeserializeOwned>(
    local_guard: &ResetRootGuard,
    local_appdata: &Path,
    path: &Path,
) -> Result<T, DiagnosticError> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;

    if path
        .parent()
        .is_none_or(|parent| !paths_equal(parent, local_appdata))
    {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    local_guard.revalidate()?;
    let metadata = fs::symlink_metadata(path).map_err(DiagnosticError::Io)?;
    if is_reparse_or_symlink(&metadata)
        || !metadata.is_file()
        || metadata.len() > MAX_RESET_STATE_BYTES
    {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)
        .map_err(DiagnosticError::Io)?;
    let opened = file.metadata().map_err(DiagnosticError::Io)?;
    if is_reparse_or_symlink(&opened) || !opened.is_file() || opened.len() > MAX_RESET_STATE_BYTES {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    let mut bytes = Vec::with_capacity(usize::try_from(opened.len()).unwrap_or_default());
    file.read_to_end(&mut bytes).map_err(DiagnosticError::Io)?;
    local_guard.revalidate()?;
    serde_json::from_slice(&bytes).map_err(|_| DiagnosticError::InvalidResetRequest)
}

#[cfg(windows)]
fn run_owned_reset_transaction<F>(
    local_appdata: &Path,
    target: &Path,
    nonce: &str,
    max_attempts: usize,
    retry_delay: Duration,
    mut before_cleanup: F,
) -> ResetWorkerOutcome
where
    F: FnMut(ResetEvent) -> Result<(), DiagnosticError>,
{
    for attempt in 0..max_attempts {
        let result =
            owned_reset_attempt(local_appdata, target, nonce, attempt, &mut before_cleanup)
                .map_err(classify_reset_error);
        match result {
            Ok(()) => return ResetWorkerOutcome::Success,
            Err(DiagnosticError::ResetLocked(_)) if attempt + 1 < max_attempts => {
                std::thread::sleep(retry_delay);
            }
            Err(DiagnosticError::ResetLocked(_)) => return ResetWorkerOutcome::RetryExhausted,
            Err(DiagnosticError::ResetTargetConflict) => {
                return ResetWorkerOutcome::TargetConflict;
            }
            Err(_) => return ResetWorkerOutcome::Failed,
        }
    }
    ResetWorkerOutcome::RetryExhausted
}

#[cfg(windows)]
fn owned_reset_attempt<F>(
    local_appdata: &Path,
    target: &Path,
    nonce: &str,
    attempt: usize,
    before_cleanup: &mut F,
) -> Result<(), DiagnosticError>
where
    F: FnMut(ResetEvent) -> Result<(), DiagnosticError>,
{
    let local_guard = ResetRootGuard::open(local_appdata)?;
    let request_path = reset_request_path(local_appdata, nonce)?;
    let owner_path = reset_owner_path(local_appdata, nonce)?;
    let deleting_path = reset_deleting_path(local_appdata, nonce)?;
    let cleaned_path = reset_cleaned_path(local_appdata, nonce)?;
    let stage_path = reset_stage_path(local_appdata, nonce)?;
    let parent_exited_path = reset_parent_exited_path(local_appdata, nonce)?;
    let commit_path = reset_commit_path(local_appdata, nonce)?;
    let quarantine_name = reset_quarantine_name(nonce)?;
    let quarantine = local_appdata.join(&quarantine_name);
    let staging_name = reset_staging_name(nonce)?;
    let staging = local_appdata.join(&staging_name);
    let request = read_authenticated_reset_request(
        &local_guard,
        local_appdata,
        &request_path,
        &commit_path,
        nonce,
    )?;

    if commit_path.exists() {
        let commit =
            read_reset_state::<ResetCommitRecord>(&local_guard, local_appdata, &commit_path)?;
        validate_commit(&commit, &request, nonce, &staging_name)?;
        let target_handle = open_verified_empty_directory(target, commit.target_identity)?;
        drop(target_handle);
        cleanup_committed_reset_state(
            &local_guard,
            [
                (ResetCleanupStep::Deleting, &deleting_path),
                (ResetCleanupStep::Deleted, &cleaned_path),
                (ResetCleanupStep::Staged, &stage_path),
                (ResetCleanupStep::Owner, &owner_path),
                (ResetCleanupStep::ParentExited, &parent_exited_path),
                (ResetCleanupStep::Request, &request_path),
                (ResetCleanupStep::Committed, &commit_path),
            ],
            before_cleanup,
        )?;
        return Ok(());
    }

    let owner = if owner_path.exists() {
        let owner =
            read_reset_state::<ResetOwnershipRecord>(&local_guard, local_appdata, &owner_path)?;
        if owner.nonce != nonce || owner.quarantine_name != quarantine_name {
            return Err(DiagnosticError::InvalidResetRequest);
        }
        owner
    } else {
        let validated = validate_reset_target(local_appdata, target)?;
        let root = WindowsFileHandle::open_for_rename(&validated)?;
        if root.is_reparse() || quarantine.exists() {
            return Err(DiagnosticError::ResetTargetConflict);
        }
        let owner = ResetOwnershipRecord {
            nonce: nonce.to_owned(),
            root_identity: root.identity,
            quarantine_name: quarantine_name.clone(),
        };
        create_reset_state(&local_guard, local_appdata, &owner_path, &owner)?;
        local_guard.revalidate()?;
        root.rename_to(&quarantine).map_err(classify_reset_error)?;
        let pinned = WindowsFileHandle::open_pinned(&quarantine)?;
        if pinned.identity != owner.root_identity || pinned.is_reparse() {
            return Err(DiagnosticError::UnsafeResetTarget);
        }
        drop(pinned);
        owner
    };

    if cleaned_path.exists() {
        let cleaned =
            read_reset_state::<ResetOwnershipRecord>(&local_guard, local_appdata, &cleaned_path)?;
        if cleaned != owner || quarantine.exists() {
            return Err(DiagnosticError::InvalidResetRequest);
        }
    } else {
        let pinned = if deleting_path.exists() {
            let deleting = read_reset_state::<ResetOwnershipRecord>(
                &local_guard,
                local_appdata,
                &deleting_path,
            )?;
            if deleting != owner {
                return Err(DiagnosticError::InvalidResetRequest);
            }
            if quarantine.exists() {
                Some(open_owned_quarantine(
                    &local_guard,
                    target,
                    &quarantine,
                    &owner,
                )?)
            } else {
                match fs::symlink_metadata(target) {
                    Ok(_) => return Err(DiagnosticError::ResetTargetConflict),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                    Err(error) => return Err(classify_reset_io(error)),
                }
            }
        } else {
            let pinned = open_owned_quarantine(&local_guard, target, &quarantine, &owner)?;
            create_reset_state(&local_guard, local_appdata, &deleting_path, &owner)?;
            Some(pinned)
        };
        if let Some(pinned) = pinned {
            before_cleanup(ResetEvent::BeforeQuarantineCleanup { attempt })?;
            remove_pinned_entry(&quarantine, pinned).map_err(classify_reset_error)?;
            before_cleanup(ResetEvent::AfterQuarantineDeletion)?;
        }
        create_reset_state(&local_guard, local_appdata, &cleaned_path, &owner)?;
    }

    let stage = if stage_path.exists() {
        let stage = read_reset_state::<ResetStageRecord>(&local_guard, local_appdata, &stage_path)?;
        if stage.nonce != nonce || stage.staging_name != staging_name {
            return Err(DiagnosticError::InvalidResetRequest);
        }
        stage
    } else {
        match fs::symlink_metadata(target) {
            Ok(_) => return Err(DiagnosticError::ResetTargetConflict),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(classify_reset_io(error)),
        }
        local_guard.revalidate()?;
        if staging.exists() {
            return Err(DiagnosticError::InvalidResetRequest);
        }
        fs::create_dir(&staging).map_err(classify_reset_io)?;
        let staged = WindowsFileHandle::open_for_rename(&staging)?;
        verify_empty_directory_handle(&staging, &staged, staged.identity)?;
        let stage = ResetStageRecord {
            nonce: nonce.to_owned(),
            staging_name: staging_name.clone(),
            staging_identity: staged.identity,
        };
        create_reset_state(&local_guard, local_appdata, &stage_path, &stage)?;
        drop(staged);
        stage
    };

    if staging.exists() {
        match fs::symlink_metadata(target) {
            Ok(_) => return Err(DiagnosticError::ResetTargetConflict),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(classify_reset_io(error)),
        }
        let staged = WindowsFileHandle::open_for_rename(&staging)?;
        verify_empty_directory_handle(&staging, &staged, stage.staging_identity)?;
        local_guard.revalidate()?;
        staged.rename_to(target).map_err(classify_reset_error)?;
        before_cleanup(ResetEvent::AfterTargetRename)?;
        let created = open_verified_empty_directory(target, stage.staging_identity)?;
        drop(created);
        drop(staged);
    } else {
        let created = open_verified_empty_directory(target, stage.staging_identity)?;
        drop(created);
    }

    let commit = ResetCommitRecord {
        request,
        staging_name,
        target_identity: stage.staging_identity,
    };
    create_reset_state(&local_guard, local_appdata, &commit_path, &commit)?;
    let target_handle = open_verified_empty_directory(target, commit.target_identity)?;
    drop(target_handle);
    cleanup_committed_reset_state(
        &local_guard,
        [
            (ResetCleanupStep::Deleting, &deleting_path),
            (ResetCleanupStep::Deleted, &cleaned_path),
            (ResetCleanupStep::Staged, &stage_path),
            (ResetCleanupStep::Owner, &owner_path),
            (ResetCleanupStep::ParentExited, &parent_exited_path),
            (ResetCleanupStep::Request, &request_path),
            (ResetCleanupStep::Committed, &commit_path),
        ],
        before_cleanup,
    )?;
    Ok(())
}

#[cfg(windows)]
fn read_authenticated_reset_request(
    local_guard: &ResetRootGuard,
    local_appdata: &Path,
    request_path: &Path,
    commit_path: &Path,
    nonce: &str,
) -> Result<ResetRequest, DiagnosticError> {
    let request = if request_path.exists() {
        read_reset_state::<ResetRequest>(local_guard, local_appdata, request_path)?
    } else if commit_path.exists() {
        read_reset_state::<ResetCommitRecord>(local_guard, local_appdata, commit_path)?.request
    } else {
        return Err(DiagnosticError::InvalidResetRequest);
    };
    if request.nonce != nonce {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    Ok(request)
}

#[cfg(windows)]
fn validate_commit(
    commit: &ResetCommitRecord,
    request: &ResetRequest,
    nonce: &str,
    staging_name: &str,
) -> Result<(), DiagnosticError> {
    if &commit.request != request
        || commit.request.nonce != nonce
        || commit.staging_name != staging_name
    {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    Ok(())
}

#[cfg(windows)]
fn open_verified_empty_directory(
    path: &Path,
    expected_identity: WindowsFileIdentity,
) -> Result<WindowsFileHandle, DiagnosticError> {
    let pinned = WindowsFileHandle::open_pinned(path)?;
    verify_empty_directory_handle(path, &pinned, expected_identity)?;
    Ok(pinned)
}

#[cfg(windows)]
fn verify_empty_directory_handle(
    path: &Path,
    handle: &WindowsFileHandle,
    expected_identity: WindowsFileIdentity,
) -> Result<(), DiagnosticError> {
    if handle.identity != expected_identity || handle.is_reparse() || !handle.is_directory() {
        return Err(DiagnosticError::ResetTargetConflict);
    }
    let mut entries = fs::read_dir(path).map_err(DiagnosticError::Io)?;
    if entries
        .next()
        .transpose()
        .map_err(DiagnosticError::Io)?
        .is_some()
    {
        return Err(DiagnosticError::ResetTargetConflict);
    }
    let (identity, attributes) = file_information(handle.handle)?;
    if identity != expected_identity
        || attributes & 0x0000_0400 != 0
        || attributes & 0x0000_0010 == 0
    {
        return Err(DiagnosticError::ResetTargetConflict);
    }
    Ok(())
}

#[cfg(windows)]
fn cleanup_committed_reset_state<F, const N: usize>(
    local_guard: &ResetRootGuard,
    state_paths: [(ResetCleanupStep, &PathBuf); N],
    before_cleanup: &mut F,
) -> Result<(), DiagnosticError>
where
    F: FnMut(ResetEvent) -> Result<(), DiagnosticError>,
{
    for (step, state_path) in state_paths {
        let metadata = match fs::symlink_metadata(state_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(classify_reset_io(error)),
        };
        if is_reparse_or_symlink(&metadata) || !metadata.is_file() {
            return Err(DiagnosticError::InvalidResetRequest);
        }
        before_cleanup(ResetEvent::BeforeStateCleanup(step))?;
        local_guard.revalidate()?;
        fs::remove_file(state_path).map_err(classify_reset_io)?;
        local_guard.revalidate()?;
    }
    Ok(())
}

#[cfg(windows)]
fn open_owned_quarantine(
    local_guard: &ResetRootGuard,
    target: &Path,
    quarantine: &Path,
    owner: &ResetOwnershipRecord,
) -> Result<WindowsFileHandle, DiagnosticError> {
    if quarantine.exists() {
        let pinned = WindowsFileHandle::open_pinned(quarantine)?;
        if pinned.identity != owner.root_identity || pinned.is_reparse() {
            return Err(DiagnosticError::UnsafeResetTarget);
        }
        return Ok(pinned);
    }
    let root = match WindowsFileHandle::open_for_rename(target) {
        Ok(root) => root,
        Err(DiagnosticError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
            return Err(DiagnosticError::InvalidResetRequest);
        }
        Err(error) => return Err(error),
    };
    if root.identity != owner.root_identity || root.is_reparse() {
        return Err(DiagnosticError::ResetTargetConflict);
    }
    local_guard.revalidate()?;
    root.rename_to(quarantine).map_err(classify_reset_error)?;
    let pinned = WindowsFileHandle::open_pinned(quarantine)?;
    if pinned.identity != owner.root_identity || pinned.is_reparse() {
        return Err(DiagnosticError::UnsafeResetTarget);
    }
    Ok(pinned)
}

#[cfg(windows)]
fn classify_reset_error(error: DiagnosticError) -> DiagnosticError {
    match error {
        DiagnosticError::Io(error) => classify_reset_io(error),
        other => other,
    }
}

#[cfg(windows)]
fn classify_reset_io(error: io::Error) -> DiagnosticError {
    if matches!(error.raw_os_error(), Some(32) | Some(33)) {
        DiagnosticError::ResetLocked(error)
    } else if error.kind() == io::ErrorKind::AlreadyExists {
        DiagnosticError::ResetTargetConflict
    } else {
        DiagnosticError::Io(error)
    }
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
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
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

    fn is_directory(&self) -> bool {
        self.attributes & 0x0000_0010 != 0
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
    #[error("application data reset is waiting for a locked file")]
    ResetLocked(#[source] io::Error),
    #[error("application data reset target was replaced")]
    ResetTargetConflict,
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
            Self::ResetLocked(_) => "RESET_LOCKED",
            Self::ResetTargetConflict => "RESET_TARGET_CONFLICT",
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
    fn worker_retries_never_adopt_a_swapped_target_or_leave_owned_data_behind() {
        let temp = tempfile::tempdir().unwrap();
        let local = temp.path().join("Local");
        let app_data = local.join(APP_DATA_DIRECTORY);
        let replacement = local.join("replacement");
        fs::create_dir_all(&app_data).unwrap();
        fs::create_dir_all(&replacement).unwrap();
        fs::write(app_data.join("owned"), b"owned").unwrap();
        fs::write(replacement.join("victim"), b"victim").unwrap();

        let nonce = "0123456789abcdef0123456789abcdef";
        let local_guard = ResetRootGuard::open(&local).unwrap();
        let request_path = reset_request_path(&local, nonce).unwrap();
        create_reset_state(
            &local_guard,
            &local,
            &request_path,
            &ResetRequest {
                nonce: nonce.into(),
                parent_pid: 123,
                parent_created: 456,
            },
        )
        .unwrap();
        let outcome =
            run_owned_reset_transaction(&local, &app_data, nonce, 3, Duration::ZERO, |event| {
                if let ResetEvent::BeforeQuarantineCleanup { attempt, .. } = event {
                    if attempt == 0 {
                        fs::rename(&replacement, &app_data).unwrap();
                    }
                    if attempt < 2 {
                        return Err(DiagnosticError::ResetLocked(io::Error::from_raw_os_error(
                            32,
                        )));
                    }
                }
                Ok(())
            });

        assert_eq!(outcome, ResetWorkerOutcome::TargetConflict);
        assert_eq!(fs::read(app_data.join("victim")).unwrap(), b"victim");
        assert!(
            !fs::read_dir(&local).unwrap().any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".com.cybereun.everyfile-reset-quarantine-")),
            "the originally owned quarantine must be fully accounted for"
        );
        assert!(reset_owner_path(&local, nonce).unwrap().is_file());
        assert!(reset_cleaned_path(&local, nonce).unwrap().is_file());
        assert!(request_path.is_file());
    }

    #[test]
    fn worker_resumes_when_quarantine_deletion_precedes_its_receipt() {
        let temp = tempfile::tempdir().unwrap();
        let local = temp.path().join("Local");
        let app_data = local.join(APP_DATA_DIRECTORY);
        fs::create_dir_all(&app_data).unwrap();
        fs::write(app_data.join("owned"), b"owned").unwrap();

        let nonce = "1123456789abcdef0123456789abcdef";
        let local_guard = ResetRootGuard::open(&local).unwrap();
        let request_path = reset_request_path(&local, nonce).unwrap();
        create_reset_state(
            &local_guard,
            &local,
            &request_path,
            &ResetRequest {
                nonce: nonce.into(),
                parent_pid: 123,
                parent_created: 456,
            },
        )
        .unwrap();
        assert_eq!(
            run_owned_reset_transaction(&local, &app_data, nonce, 1, Duration::ZERO, |event| {
                if matches!(event, ResetEvent::AfterQuarantineDeletion) {
                    return Err(DiagnosticError::Io(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "injected crash after quarantine deletion",
                    )));
                }
                Ok(())
            },),
            ResetWorkerOutcome::Failed
        );
        assert!(reset_deleting_path(&local, nonce).unwrap().is_file());
        assert!(!reset_cleaned_path(&local, nonce).unwrap().exists());
        assert!(!local.join(reset_quarantine_name(nonce).unwrap()).exists());

        assert_eq!(
            run_owned_reset_transaction(&local, &app_data, nonce, 1, Duration::ZERO, |_| Ok(()),),
            ResetWorkerOutcome::Success
        );
        assert!(app_data.is_dir());
        assert_eq!(fs::read_dir(&app_data).unwrap().count(), 0);
    }

    #[test]
    fn worker_resumes_after_target_rename_and_partial_state_cleanup() {
        use std::collections::VecDeque;

        let temp = tempfile::tempdir().unwrap();
        let local = temp.path().join("Local");
        let app_data = local.join(APP_DATA_DIRECTORY);
        let victim = local.join("victim");
        fs::create_dir_all(&app_data).unwrap();
        fs::create_dir_all(&victim).unwrap();
        fs::write(app_data.join("owned"), b"owned").unwrap();
        fs::write(victim.join("must-survive"), b"victim").unwrap();

        let nonce = "2123456789abcdef0123456789abcdef";
        let local_guard = ResetRootGuard::open(&local).unwrap();
        let request_path = reset_request_path(&local, nonce).unwrap();
        let request = ResetRequest {
            nonce: nonce.into(),
            parent_pid: 123,
            parent_created: 456,
        };
        create_reset_state(&local_guard, &local, &request_path, &request).unwrap();
        create_reset_state(
            &local_guard,
            &local,
            &reset_parent_exited_path(&local, nonce).unwrap(),
            &request,
        )
        .unwrap();
        let mut fail_after_rename = true;
        let mut cleanup_faults = VecDeque::from([
            ResetCleanupStep::Deleting,
            ResetCleanupStep::Deleted,
            ResetCleanupStep::Staged,
            ResetCleanupStep::Owner,
            ResetCleanupStep::ParentExited,
            ResetCleanupStep::Request,
            ResetCleanupStep::Committed,
        ]);
        let mut successes = 0;
        for _ in 0..9 {
            let outcome =
                run_owned_reset_transaction(&local, &app_data, nonce, 1, Duration::ZERO, |event| {
                    let should_fail = match event {
                        ResetEvent::AfterTargetRename if fail_after_rename => {
                            fail_after_rename = false;
                            true
                        }
                        ResetEvent::BeforeStateCleanup(step)
                            if cleanup_faults.front() == Some(&step) =>
                        {
                            cleanup_faults.pop_front();
                            true
                        }
                        _ => false,
                    };
                    if should_fail {
                        return Err(DiagnosticError::Io(io::Error::new(
                            io::ErrorKind::Interrupted,
                            "injected reset transaction interruption",
                        )));
                    }
                    Ok(())
                });
            if outcome == ResetWorkerOutcome::Success {
                successes += 1;
            } else {
                assert_eq!(outcome, ResetWorkerOutcome::Failed);
            }
        }

        assert_eq!(successes, 1);
        assert!(!fail_after_rename);
        assert!(cleanup_faults.is_empty());
        assert!(app_data.is_dir());
        assert_eq!(fs::read_dir(&app_data).unwrap().count(), 0);
        assert_eq!(fs::read(victim.join("must-survive")).unwrap(), b"victim");
        assert!(!fs::read_dir(&local).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(&format!(".{APP_DATA_DIRECTORY}-reset-"))));
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
