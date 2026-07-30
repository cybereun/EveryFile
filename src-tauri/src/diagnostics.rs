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
                if slash == display {
                    vec![display]
                } else {
                    vec![display, slash]
                }
            })
            .filter(|root| !root.is_empty())
            .collect();
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
            redacted = replace_case_insensitive(&redacted, root, "[REGISTERED_ROOT]");
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

fn replace_case_insensitive(haystack: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return haystack.to_owned();
    }
    let mut output = String::with_capacity(haystack.len());
    let mut byte_offset = 0;
    while byte_offset < haystack.len() {
        let remaining = &haystack[byte_offset..];
        let found = remaining.char_indices().find_map(|(relative, _)| {
            let start = byte_offset + relative;
            let end = start.checked_add(needle.len())?;
            (end <= haystack.len()
                && haystack.is_char_boundary(end)
                && haystack[start..end].eq_ignore_ascii_case(needle))
            .then_some(start)
        });
        let Some(found) = found else {
            break;
        };
        output.push_str(&haystack[byte_offset..found]);
        output.push_str(replacement);
        byte_offset = found + needle.len();
    }
    output.push_str(&haystack[byte_offset..]);
    output
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
    for entry in fs::read_dir(&target).map_err(DiagnosticError::Io)? {
        let entry = entry.map_err(DiagnosticError::Io)?;
        remove_entry_without_following(&entry.path())?;
    }
    Ok(())
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
    let request_path = reset_request_path(&app_data_dir, &nonce)?;
    let mut request_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&request_path)
        .map_err(DiagnosticError::Io)?;
    writeln!(request_file, "{nonce}\n{parent_pid}").map_err(DiagnosticError::Io)?;
    request_file.sync_all().map_err(DiagnosticError::Io)?;
    drop(request_file);
    let executable = std::env::current_exe().map_err(DiagnosticError::Io)?;
    let spawn = Command::new(executable)
        .arg("--everyfile-reset-after-exit")
        .arg(&nonce)
        .arg(parent_pid.to_string())
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

        if arguments.len() != 4 {
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
        let Some(local_appdata) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) else {
            return true;
        };
        let target = local_appdata.join(APP_DATA_DIRECTORY);
        let Ok(target) = validate_reset_target(&local_appdata, &target) else {
            return true;
        };
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
        if request != format!("{nonce}\n{parent_pid}\n") {
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

fn remove_entry_without_following(path: &Path) -> Result<(), DiagnosticError> {
    let metadata = fs::symlink_metadata(path).map_err(DiagnosticError::Io)?;
    if metadata.file_type().is_symlink() {
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
