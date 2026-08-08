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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ResetRequest {
    nonce: String,
    parent_pid: u32,
    parent_created: u64,
}

#[cfg(windows)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ResetOwnershipRecord {
    nonce: String,
    root_identity: WindowsFileIdentity,
    quarantine_name: String,
}

#[cfg(windows)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ResetStageRecord {
    nonce: String,
    staging_name: String,
    staging_identity: WindowsFileIdentity,
}

#[cfg(windows)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ResetCommitRecord {
    request: ResetRequest,
    staging_name: String,
    target_identity: WindowsFileIdentity,
}

#[cfg(windows)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ResetCompletionRecord {
    commit: ResetCommitRecord,
}

#[cfg(windows)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ResetStartupRecord {
    completion: ResetCompletionRecord,
    child_pid: u32,
    child_created: u64,
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
    Completed,
    Startup,
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResetStateStep {
    Request,
    ParentExited,
    Owner,
    Staged,
    Deleting,
    Deleted,
    Committed,
    Completed,
    Startup,
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResetStateWritePoint {
    AfterTempCreate,
    AfterPartialWrite,
    AfterTempSync,
    BeforeRename,
    AfterRename,
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResetRenameStep {
    Staging,
    Quarantine,
    Target,
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
enum ResetEvent {
    BeforeStageDirectoryCreate,
    AfterStageDirectoryCreate,
    BeforeStateWrite(ResetStateStep),
    AfterStateWrite(ResetStateStep),
    StateWritePoint {
        step: ResetStateStep,
        point: ResetStateWritePoint,
    },
    BeforeRename(ResetRenameStep),
    AfterRename(ResetRenameStep),
    BeforeQuarantineCleanup {
        attempt: usize,
    },
    AfterQuarantineDeletion,
    BeforeStateCleanup(ResetCleanupStep),
    AfterStateCleanup(ResetCleanupStep),
    BeforeRestartSpawn,
    AfterRestartSpawn,
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
    let mut no_event = |_| Ok(());
    create_reset_state(
        &local_guard,
        &local_appdata,
        &request_path,
        &ResetRequest {
            nonce: nonce.clone(),
            parent_pid,
            parent_created,
        },
        &nonce,
        ResetStateStep::Request,
        &mut no_event,
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
        let _ = local_guard.sync_directory_supported();
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
        let request = ResetRequest {
            nonce: nonce.to_owned(),
            parent_pid,
            parent_created,
        };
        let mut wait_for_parent = wait_for_exact_process_exit;
        for _ in 0..16 {
            let mut spawned_child = None;
            let mut process_alive = exact_process_is_alive;
            let outcome = {
                let mut spawn_restart = |completion_nonce: &str| {
                    let executable = std::env::current_exe().map_err(DiagnosticError::Io)?;
                    let child = Command::new(executable)
                        .arg("--reset-completed")
                        .arg(completion_nonce)
                        .creation_flags(0x0800_0000)
                        .spawn()
                        .map_err(DiagnosticError::Io)?;
                    spawned_child = Some(child);
                    Ok(())
                };
                let mut on_event = |_| Ok(());
                run_authenticated_reset_worker_and_restart(
                    &local_appdata,
                    nonce,
                    &request,
                    300,
                    Duration::from_millis(100),
                    &mut wait_for_parent,
                    &mut process_alive,
                    &mut spawn_restart,
                    &mut on_event,
                )
            };
            if outcome != ResetWorkerOutcome::Success {
                break;
            }
            match wait_for_reset_startup_result(
                &local_appdata,
                nonce,
                spawned_child.as_mut(),
                6_000,
                Duration::from_millis(50),
            ) {
                Ok(ResetStartupWait::Completed) => break,
                Ok(ResetStartupWait::FailedChild) => continue,
                Ok(ResetStartupWait::TimedOut) | Err(_) => break,
            }
        }
    }
    true
}

#[cfg(windows)]
fn run_authenticated_reset_worker<W, F>(
    local_appdata: &Path,
    nonce: &str,
    expected_request: &ResetRequest,
    max_attempts: usize,
    retry_delay: Duration,
    wait_for_parent: &mut W,
    on_event: &mut F,
) -> ResetWorkerOutcome
where
    W: FnMut(u32, u64) -> Result<(), DiagnosticError>,
    F: FnMut(ResetEvent) -> Result<(), DiagnosticError>,
{
    let result = (|| {
        let target = local_appdata.join(APP_DATA_DIRECTORY);
        let request_path = reset_request_path(local_appdata, nonce)?;
        let commit_path = reset_commit_path(local_appdata, nonce)?;
        let completion_path = reset_completion_path(local_appdata, nonce)?;
        let startup_path = reset_startup_path(local_appdata, nonce)?;
        let parent_exited_path = reset_parent_exited_path(local_appdata, nonce)?;
        let local_guard = ResetRootGuard::open(local_appdata)?;
        let request = read_authenticated_reset_request(
            &local_guard,
            local_appdata,
            &request_path,
            &commit_path,
            nonce,
        )?;
        if &request != expected_request {
            return Err(DiagnosticError::InvalidResetRequest);
        }

        if commit_path.exists() || completion_path.exists() || startup_path.exists() {
            // A valid commit can only be written after this helper has authenticated and
            // observed the exact originating process exit. Keep it as the durable proof
            // while earlier receipts are removed during resumable cleanup.
        } else if parent_exited_path.exists() {
            let recorded =
                read_reset_state::<ResetRequest>(&local_guard, local_appdata, &parent_exited_path)?;
            if recorded != request {
                return Err(DiagnosticError::InvalidResetRequest);
            }
        } else {
            wait_for_parent(request.parent_pid, request.parent_created)?;
            write_reset_state_with_events(
                &local_guard,
                local_appdata,
                &parent_exited_path,
                &request,
                nonce,
                ResetStateStep::ParentExited,
                on_event,
            )?;
        }

        Ok(run_owned_reset_transaction(
            local_appdata,
            &target,
            nonce,
            max_attempts,
            retry_delay,
            on_event,
        ))
    })();

    match result {
        Ok(outcome) => outcome,
        Err(DiagnosticError::ResetTargetConflict) => ResetWorkerOutcome::TargetConflict,
        Err(DiagnosticError::ResetLocked(_)) => ResetWorkerOutcome::RetryExhausted,
        Err(_) => ResetWorkerOutcome::Failed,
    }
}

#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
fn run_authenticated_reset_worker_and_restart<W, A, S, F>(
    local_appdata: &Path,
    nonce: &str,
    expected_request: &ResetRequest,
    max_attempts: usize,
    retry_delay: Duration,
    wait_for_parent: &mut W,
    process_alive: &mut A,
    spawn_restart: &mut S,
    on_event: &mut F,
) -> ResetWorkerOutcome
where
    W: FnMut(u32, u64) -> Result<(), DiagnosticError>,
    A: FnMut(u32, u64) -> Result<bool, DiagnosticError>,
    S: FnMut(&str) -> Result<(), DiagnosticError>,
    F: FnMut(ResetEvent) -> Result<(), DiagnosticError>,
{
    let outcome = run_authenticated_reset_worker(
        local_appdata,
        nonce,
        expected_request,
        max_attempts,
        retry_delay,
        wait_for_parent,
        on_event,
    );
    if outcome != ResetWorkerOutcome::Success {
        return outcome;
    }

    let result = (|| {
        let local_guard = ResetRootGuard::open(local_appdata)?;
        let completion_path = reset_completion_path(local_appdata, nonce)?;
        let startup_path = reset_startup_path(local_appdata, nonce)?;
        let completion =
            read_terminal_completion(&local_guard, local_appdata, &completion_path, &startup_path)?;
        validate_completion(&completion, expected_request, nonce)?;
        let target = local_appdata.join(APP_DATA_DIRECTORY);
        let target_handle =
            open_verified_directory_identity(&target, completion.commit.target_identity)?;
        drop(target_handle);

        if startup_path.exists() {
            let startup =
                read_reset_state::<ResetStartupRecord>(&local_guard, local_appdata, &startup_path)?;
            validate_startup(&startup, &completion, nonce)?;
            if process_alive(startup.child_pid, startup.child_created)? {
                return Ok(());
            }
            if !completion_path.exists() {
                write_reset_state_with_events(
                    &local_guard,
                    local_appdata,
                    &completion_path,
                    &completion,
                    nonce,
                    ResetStateStep::Completed,
                    on_event,
                )?;
            }
            cleanup_committed_reset_state(
                &local_guard,
                [(ResetCleanupStep::Startup, &startup_path)],
                on_event,
            )?;
        }

        on_event(ResetEvent::BeforeRestartSpawn)?;
        spawn_restart(nonce)?;
        on_event(ResetEvent::AfterRestartSpawn)
    })();

    match result {
        Ok(()) => ResetWorkerOutcome::Success,
        Err(DiagnosticError::ResetTargetConflict) => ResetWorkerOutcome::TargetConflict,
        Err(DiagnosticError::ResetLocked(_)) => ResetWorkerOutcome::RetryExhausted,
        Err(_) => ResetWorkerOutcome::Failed,
    }
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResetStartupWait {
    Completed,
    FailedChild,
    TimedOut,
}

#[cfg(windows)]
fn wait_for_reset_startup_result(
    local_appdata: &Path,
    nonce: &str,
    mut spawned_child: Option<&mut std::process::Child>,
    max_polls: usize,
    poll_delay: Duration,
) -> Result<ResetStartupWait, DiagnosticError> {
    let completion_path = reset_completion_path(local_appdata, nonce)?;
    let startup_path = reset_startup_path(local_appdata, nonce)?;
    for _ in 0..max_polls {
        if !completion_path.exists() && !startup_path.exists() {
            return Ok(ResetStartupWait::Completed);
        }
        if startup_path.exists() {
            let local_guard = ResetRootGuard::open(local_appdata)?;
            let completion = match read_terminal_completion(
                &local_guard,
                local_appdata,
                &completion_path,
                &startup_path,
            ) {
                Ok(completion) => completion,
                Err(DiagnosticError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
                    std::thread::sleep(poll_delay);
                    continue;
                }
                Err(error) => return Err(error),
            };
            let startup = match read_reset_state::<ResetStartupRecord>(
                &local_guard,
                local_appdata,
                &startup_path,
            ) {
                Ok(startup) => startup,
                Err(DiagnosticError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
                    std::thread::sleep(poll_delay);
                    continue;
                }
                Err(error) => return Err(error),
            };
            validate_startup(&startup, &completion, nonce)?;
            if !exact_process_is_alive(startup.child_pid, startup.child_created)? {
                return Ok(ResetStartupWait::FailedChild);
            }
        } else if let Some(child) = spawned_child.as_deref_mut() {
            if child.try_wait().map_err(DiagnosticError::Io)?.is_some() {
                return Ok(ResetStartupWait::FailedChild);
            }
        }
        std::thread::sleep(poll_delay);
    }
    Ok(ResetStartupWait::TimedOut)
}

#[cfg(windows)]
#[derive(Clone)]
pub struct ResetCompletionStartup {
    local_appdata: PathBuf,
    nonce: String,
    startup: ResetStartupRecord,
}

#[cfg(not(windows))]
pub struct ResetCompletionStartup;

#[cfg(windows)]
pub fn prepare_reset_completion_from_args(
) -> Result<Option<ResetCompletionStartup>, DiagnosticError> {
    let arguments = std::env::args_os().collect::<Vec<_>>();
    if arguments.get(1).map(std::ffi::OsString::as_os_str)
        != Some(std::ffi::OsStr::new("--reset-completed"))
    {
        return Ok(None);
    }
    if arguments.len() != 3 {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    let nonce = arguments[2]
        .to_str()
        .filter(|nonce| valid_reset_nonce(nonce))
        .ok_or(DiagnosticError::InvalidResetRequest)?;
    let local_appdata = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or(DiagnosticError::LocalAppDataUnavailable)?;
    let child_pid = std::process::id();
    let child_created = current_process_creation_identity()?;
    let mut process_alive = exact_process_is_alive;
    let mut no_event = |_| Ok(());
    prepare_reset_completion(
        &local_appdata,
        nonce,
        child_pid,
        child_created,
        &mut process_alive,
        &mut no_event,
    )
    .map(Some)
}

#[cfg(not(windows))]
pub fn prepare_reset_completion_from_args(
) -> Result<Option<ResetCompletionStartup>, DiagnosticError> {
    Ok(None)
}

#[cfg(windows)]
fn prepare_reset_completion<A, F>(
    local_appdata: &Path,
    nonce: &str,
    child_pid: u32,
    child_created: u64,
    process_alive: &mut A,
    on_event: &mut F,
) -> Result<ResetCompletionStartup, DiagnosticError>
where
    A: FnMut(u32, u64) -> Result<bool, DiagnosticError>,
    F: FnMut(ResetEvent) -> Result<(), DiagnosticError>,
{
    if child_pid == 0 || child_created == 0 {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    let local_guard = ResetRootGuard::open(local_appdata)?;
    let completion_path = reset_completion_path(local_appdata, nonce)?;
    let startup_path = reset_startup_path(local_appdata, nonce)?;
    let completion =
        read_terminal_completion(&local_guard, local_appdata, &completion_path, &startup_path)?;
    let request = completion.commit.request.clone();
    validate_completion(&completion, &request, nonce)?;
    let target = local_appdata.join(APP_DATA_DIRECTORY);
    let target_handle =
        open_verified_directory_identity(&target, completion.commit.target_identity)?;

    let mut resumed_startup = false;
    if startup_path.exists() {
        let existing =
            read_reset_state::<ResetStartupRecord>(&local_guard, local_appdata, &startup_path)?;
        validate_startup(&existing, &completion, nonce)?;
        if existing.child_pid == child_pid && existing.child_created == child_created {
            drop(target_handle);
            return Ok(ResetCompletionStartup {
                local_appdata: local_appdata.to_path_buf(),
                nonce: nonce.to_owned(),
                startup: existing,
            });
        }
        if process_alive(existing.child_pid, existing.child_created)? {
            return Err(DiagnosticError::InvalidResetRequest);
        }
        if !completion_path.exists() {
            write_reset_state_with_events(
                &local_guard,
                local_appdata,
                &completion_path,
                &completion,
                nonce,
                ResetStateStep::Completed,
                on_event,
            )?;
        }
        validate_terminal_transaction_markers(&local_guard, local_appdata, nonce, &completion)?;
        cleanup_committed_reset_state(
            &local_guard,
            [(ResetCleanupStep::Startup, &startup_path)],
            on_event,
        )?;
        resumed_startup = true;
    }
    if !resumed_startup {
        verify_empty_directory_handle(&target, &target_handle, completion.commit.target_identity)?;
    }
    drop(target_handle);

    let startup = ResetStartupRecord {
        completion,
        child_pid,
        child_created,
    };
    write_reset_state_with_events(
        &local_guard,
        local_appdata,
        &startup_path,
        &startup,
        nonce,
        ResetStateStep::Startup,
        on_event,
    )?;
    Ok(ResetCompletionStartup {
        local_appdata: local_appdata.to_path_buf(),
        nonce: nonce.to_owned(),
        startup,
    })
}

#[cfg(windows)]
pub fn finish_reset_completion_startup(
    startup: ResetCompletionStartup,
) -> Result<(), DiagnosticError> {
    let mut no_event = |_| Ok(());
    complete_reset_startup(startup, &mut no_event)
}

#[cfg(not(windows))]
pub fn finish_reset_completion_startup(
    _startup: ResetCompletionStartup,
) -> Result<(), DiagnosticError> {
    Ok(())
}

#[cfg(windows)]
fn complete_reset_startup<F>(
    startup: ResetCompletionStartup,
    on_event: &mut F,
) -> Result<(), DiagnosticError>
where
    F: FnMut(ResetEvent) -> Result<(), DiagnosticError>,
{
    let local_guard = ResetRootGuard::open(&startup.local_appdata)?;
    let completion_path = reset_completion_path(&startup.local_appdata, &startup.nonce)?;
    let startup_path = reset_startup_path(&startup.local_appdata, &startup.nonce)?;
    let stored_startup = read_reset_state::<ResetStartupRecord>(
        &local_guard,
        &startup.local_appdata,
        &startup_path,
    )?;
    if stored_startup != startup.startup {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    let completion = read_terminal_completion(
        &local_guard,
        &startup.local_appdata,
        &completion_path,
        &startup_path,
    )?;
    validate_startup(&stored_startup, &completion, &startup.nonce)?;
    let target = startup.local_appdata.join(APP_DATA_DIRECTORY);
    let target_handle =
        open_verified_directory_identity(&target, completion.commit.target_identity)?;
    drop(target_handle);

    cleanup_committed_reset_state(
        &local_guard,
        [
            (ResetCleanupStep::Completed, &completion_path),
            (ResetCleanupStep::Startup, &startup_path),
        ],
        on_event,
    )
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
fn exact_process_is_alive(
    process_id: u32,
    expected_creation: u64,
) -> Result<bool, DiagnosticError> {
    use windows::Win32::Foundation::{CloseHandle, FILETIME, WAIT_OBJECT_0, WAIT_TIMEOUT};
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
                return Ok(false);
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
            return Ok(false);
        }
        match unsafe { WaitForSingleObject(process, 0) } {
            WAIT_TIMEOUT => Ok(true),
            WAIT_OBJECT_0 => Ok(false),
            _ => Err(DiagnosticError::InvalidResetRequest),
        }
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
fn reset_completion_path(local_appdata: &Path, nonce: &str) -> Result<PathBuf, DiagnosticError> {
    if !valid_reset_nonce(nonce) {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    Ok(local_appdata.join(format!(
        ".{APP_DATA_DIRECTORY}-reset-completed-{nonce}.json"
    )))
}

#[cfg(windows)]
fn reset_startup_path(local_appdata: &Path, nonce: &str) -> Result<PathBuf, DiagnosticError> {
    if !valid_reset_nonce(nonce) {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    Ok(local_appdata.join(format!(".{APP_DATA_DIRECTORY}-reset-startup-{nonce}.json")))
}

#[cfg(windows)]
fn reset_quarantine_name(nonce: &str) -> Result<String, DiagnosticError> {
    if !valid_reset_nonce(nonce) {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    Ok(format!(".{APP_DATA_DIRECTORY}-reset-quarantine-{nonce}"))
}

#[cfg(windows)]
fn new_reset_staging_name(nonce: &str) -> Result<String, DiagnosticError> {
    if !valid_reset_nonce(nonce) {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    Ok(format!(
        ".{APP_DATA_DIRECTORY}-reset-empty-{nonce}-{:032x}",
        rand::rng().random::<u128>()
    ))
}

#[cfg(windows)]
fn valid_reset_staging_name(nonce: &str, name: &str) -> bool {
    let prefix = format!(".{APP_DATA_DIRECTORY}-reset-empty-{nonce}-");
    let Some(suffix) = name.strip_prefix(&prefix) else {
        return false;
    };
    suffix.len() == 32
        && suffix
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        && Path::new(name).components().count() == 1
}

#[cfg(windows)]
fn reset_state_step_tag(step: ResetStateStep) -> &'static str {
    match step {
        ResetStateStep::Request => "request",
        ResetStateStep::ParentExited => "parent-exited",
        ResetStateStep::Owner => "owner",
        ResetStateStep::Staged => "staged",
        ResetStateStep::Deleting => "deleting",
        ResetStateStep::Deleted => "deleted",
        ResetStateStep::Committed => "committed",
        ResetStateStep::Completed => "completed",
        ResetStateStep::Startup => "startup",
    }
}

#[cfg(windows)]
fn reset_state_temp_path(
    local_appdata: &Path,
    nonce: &str,
    step: ResetStateStep,
) -> Result<PathBuf, DiagnosticError> {
    if !valid_reset_nonce(nonce) {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    Ok(local_appdata.join(format!(
        ".{APP_DATA_DIRECTORY}-reset-state-temp-{nonce}-{}-{:032x}.tmp",
        reset_state_step_tag(step),
        rand::rng().random::<u128>()
    )))
}

#[cfg(windows)]
#[cfg(windows)]
fn create_reset_state<T, F>(
    local_guard: &ResetRootGuard,
    local_appdata: &Path,
    path: &Path,
    state: &T,
    nonce: &str,
    step: ResetStateStep,
    on_event: &mut F,
) -> Result<(), DiagnosticError>
where
    T: Serialize,
    F: FnMut(ResetEvent) -> Result<(), DiagnosticError>,
{
    write_reset_state_with_events(
        local_guard,
        local_appdata,
        path,
        state,
        nonce,
        step,
        on_event,
    )
}

#[cfg(windows)]
#[cfg(test)]
fn report_reset_test_error(stage: &str, error: &DiagnosticError) {
    eprintln!("reset test stage {stage}: {error:?}");
}

#[cfg(windows)]
fn write_reset_state_with_events<T, F>(
    local_guard: &ResetRootGuard,
    local_appdata: &Path,
    path: &Path,
    state: &T,
    nonce: &str,
    step: ResetStateStep,
    on_event: &mut F,
) -> Result<(), DiagnosticError>
where
    T: Serialize,
    F: FnMut(ResetEvent) -> Result<(), DiagnosticError>,
{
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        DELETE, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_WRITE, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    if path
        .parent()
        .is_none_or(|parent| !paths_equal(parent, local_appdata))
        || !valid_reset_nonce(nonce)
    {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    let bytes = serde_json::to_vec(state).map_err(DiagnosticError::Serialize)?;
    if bytes.len() < 2 || u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_RESET_STATE_BYTES {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    on_event(ResetEvent::BeforeStateWrite(step))?;
    local_guard.revalidate()?;
    if path.exists() {
        return Err(DiagnosticError::Io(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "reset state marker already exists",
        )));
    }
    let temporary = reset_state_temp_path(local_appdata, nonce, step)?;
    let mut file = match OpenOptions::new()
        .write(true)
        .access_mode((FILE_GENERIC_WRITE | DELETE).0)
        // The state marker is atomically renamed while this handle is still open.
        // Windows requires the handle's share mode to permit delete/rename; this
        // is enforced more strictly on the hosted Windows runners than on some
        // desktop versions and otherwise surfaces as ERROR_FILE_NOT_FOUND from
        // SetFileInformationByHandle.
        .share_mode((FILE_SHARE_DELETE | FILE_SHARE_READ | FILE_SHARE_WRITE).0)
        .create_new(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(&temporary)
    {
        Ok(file) => file,
        Err(error) => {
            let error = DiagnosticError::Io(error);
            #[cfg(test)]
            report_reset_test_error("temp-open", &error);
            return Err(error);
        }
    };
    on_event(ResetEvent::StateWritePoint {
        step,
        point: ResetStateWritePoint::AfterTempCreate,
    })?;
    let raw_handle = HANDLE(file.as_raw_handle());
    let split = (bytes.len() / 2).clamp(1, bytes.len() - 1);
    file.write_all(&bytes[..split])
        .map_err(DiagnosticError::Io)?;
    on_event(ResetEvent::StateWritePoint {
        step,
        point: ResetStateWritePoint::AfterPartialWrite,
    })?;
    file.write_all(&bytes[split..])
        .map_err(DiagnosticError::Io)?;
    let result = file.sync_all().map_err(DiagnosticError::Io);
    #[cfg(test)]
    if let Err(ref error) = result {
        report_reset_test_error("temp-sync", error);
    }
    result?;
    on_event(ResetEvent::StateWritePoint {
        step,
        point: ResetStateWritePoint::AfterTempSync,
    })?;
    let (written_identity, attributes) = file_information(raw_handle)?;
    if attributes & 0x0000_0400 != 0 || attributes & 0x0000_0010 != 0 {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    local_guard.revalidate()?;
    on_event(ResetEvent::StateWritePoint {
        step,
        point: ResetStateWritePoint::BeforeRename,
    })?;
    let result = rename_windows_handle_to(raw_handle, path).map_err(classify_reset_error);
    #[cfg(test)]
    if let Err(ref error) = result {
        report_reset_test_error("rename", error);
    }
    result?;
    on_event(ResetEvent::StateWritePoint {
        step,
        point: ResetStateWritePoint::AfterRename,
    })?;
    let result = local_guard.sync_directory_supported();
    #[cfg(test)]
    if let Err(ref error) = result {
        report_reset_test_error("directory-sync-after-rename", error);
    }
    result?;
    let published = match WindowsFileHandle::open_for_identity(path) {
        Ok(file) => file,
        Err(error) => {
            #[cfg(test)]
            report_reset_test_error("published-open", &error);
            return Err(error);
        }
    };
    if published.identity != written_identity || published.is_reparse() || published.is_directory()
    {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    drop(published);
    local_guard.revalidate()?;
    let result = file.sync_all().map_err(DiagnosticError::Io);
    #[cfg(test)]
    if let Err(ref error) = result {
        report_reset_test_error("published-sync", error);
    }
    result?;
    let result = local_guard.sync_directory_supported();
    #[cfg(test)]
    if let Err(ref error) = result {
        report_reset_test_error("directory-sync-final", error);
    }
    result?;
    local_guard.revalidate()?;
    drop(file);
    on_event(ResetEvent::AfterStateWrite(step))
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
    mut on_event: F,
) -> ResetWorkerOutcome
where
    F: FnMut(ResetEvent) -> Result<(), DiagnosticError>,
{
    for attempt in 0..max_attempts {
        let result = owned_reset_attempt(local_appdata, target, nonce, attempt, &mut on_event)
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
    on_event: &mut F,
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
    let completion_path = reset_completion_path(local_appdata, nonce)?;
    let startup_path = reset_startup_path(local_appdata, nonce)?;
    let quarantine_name = reset_quarantine_name(nonce)?;
    let quarantine = local_appdata.join(&quarantine_name);
    let request = read_authenticated_reset_request(
        &local_guard,
        local_appdata,
        &request_path,
        &commit_path,
        nonce,
    )?;

    if completion_path.exists() || startup_path.exists() {
        let completion =
            read_terminal_completion(&local_guard, local_appdata, &completion_path, &startup_path)?;
        validate_completion(&completion, &request, nonce)?;
        let target_handle =
            open_verified_directory_identity(target, completion.commit.target_identity)?;
        drop(target_handle);
        if !completion_path.exists() {
            write_reset_state_with_events(
                &local_guard,
                local_appdata,
                &completion_path,
                &completion,
                nonce,
                ResetStateStep::Completed,
                on_event,
            )?;
        }
        validate_terminal_transaction_markers(&local_guard, local_appdata, nonce, &completion)?;
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
            on_event,
        )?;
        return Ok(());
    }

    if commit_path.exists() {
        let commit =
            read_reset_state::<ResetCommitRecord>(&local_guard, local_appdata, &commit_path)?;
        validate_commit(&commit, &request, nonce)?;
        let target_handle = open_verified_empty_directory(target, commit.target_identity)?;
        drop(target_handle);
        let completion = ResetCompletionRecord {
            commit: commit.clone(),
        };
        write_reset_state_with_events(
            &local_guard,
            local_appdata,
            &completion_path,
            &completion,
            nonce,
            ResetStateStep::Completed,
            on_event,
        )?;
        validate_terminal_transaction_markers(&local_guard, local_appdata, nonce, &completion)?;
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
            on_event,
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
        write_reset_state_with_events(
            &local_guard,
            local_appdata,
            &owner_path,
            &owner,
            nonce,
            ResetStateStep::Owner,
            on_event,
        )?;
        drop(root);
        owner
    };

    let prepared_stage = load_or_prepare_reset_stage(
        &local_guard,
        local_appdata,
        target,
        &quarantine,
        nonce,
        &owner,
        &stage_path,
        on_event,
    )?;

    if cleaned_path.exists() {
        let cleaned =
            read_reset_state::<ResetOwnershipRecord>(&local_guard, local_appdata, &cleaned_path)?;
        if cleaned != owner || quarantine.exists() {
            return Err(DiagnosticError::InvalidResetRequest);
        }
    } else {
        if prepared_stage.at_target {
            return Err(DiagnosticError::InvalidResetRequest);
        }
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
                    on_event,
                )?)
            } else {
                match fs::symlink_metadata(target) {
                    Ok(_) => return Err(DiagnosticError::ResetTargetConflict),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                    Err(error) => return Err(classify_reset_io(error)),
                }
            }
        } else {
            let pinned =
                open_owned_quarantine(&local_guard, target, &quarantine, &owner, on_event)?;
            write_reset_state_with_events(
                &local_guard,
                local_appdata,
                &deleting_path,
                &owner,
                nonce,
                ResetStateStep::Deleting,
                on_event,
            )?;
            Some(pinned)
        };
        if let Some(pinned) = pinned {
            on_event(ResetEvent::BeforeQuarantineCleanup { attempt })?;
            remove_pinned_entry(&quarantine, pinned).map_err(classify_reset_error)?;
            on_event(ResetEvent::AfterQuarantineDeletion)?;
        }
        write_reset_state_with_events(
            &local_guard,
            local_appdata,
            &cleaned_path,
            &owner,
            nonce,
            ResetStateStep::Deleted,
            on_event,
        )?;
    }

    let target_handle = if prepared_stage.at_target {
        prepared_stage.handle
    } else {
        match fs::symlink_metadata(target) {
            Ok(_) => return Err(DiagnosticError::ResetTargetConflict),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(classify_reset_io(error)),
        }
        local_guard.revalidate()?;
        on_event(ResetEvent::BeforeRename(ResetRenameStep::Target))?;
        prepared_stage
            .handle
            .rename_to(target)
            .map_err(classify_reset_error)?;
        on_event(ResetEvent::AfterRename(ResetRenameStep::Target))?;
        let created =
            open_verified_empty_directory(target, prepared_stage.record.staging_identity)?;
        drop(created);
        prepared_stage.handle
    };
    verify_empty_directory_handle(
        target,
        &target_handle,
        prepared_stage.record.staging_identity,
    )?;

    let commit = ResetCommitRecord {
        request,
        staging_name: prepared_stage.record.staging_name,
        target_identity: prepared_stage.record.staging_identity,
    };
    write_reset_state_with_events(
        &local_guard,
        local_appdata,
        &commit_path,
        &commit,
        nonce,
        ResetStateStep::Committed,
        on_event,
    )?;
    let target_handle = open_verified_empty_directory(target, commit.target_identity)?;
    drop(target_handle);
    let completion = ResetCompletionRecord {
        commit: commit.clone(),
    };
    write_reset_state_with_events(
        &local_guard,
        local_appdata,
        &completion_path,
        &completion,
        nonce,
        ResetStateStep::Completed,
        on_event,
    )?;
    validate_terminal_transaction_markers(&local_guard, local_appdata, nonce, &completion)?;
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
        on_event,
    )?;
    Ok(())
}

#[cfg(windows)]
struct PreparedResetStage {
    record: ResetStageRecord,
    handle: WindowsFileHandle,
    at_target: bool,
}

#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
fn load_or_prepare_reset_stage<F>(
    local_guard: &ResetRootGuard,
    local_appdata: &Path,
    target: &Path,
    quarantine: &Path,
    nonce: &str,
    owner: &ResetOwnershipRecord,
    stage_path: &Path,
    on_event: &mut F,
) -> Result<PreparedResetStage, DiagnosticError>
where
    F: FnMut(ResetEvent) -> Result<(), DiagnosticError>,
{
    if stage_path.exists() {
        let record = read_reset_state::<ResetStageRecord>(local_guard, local_appdata, stage_path)?;
        if record.nonce != nonce || !valid_reset_staging_name(nonce, &record.staging_name) {
            return Err(DiagnosticError::InvalidResetRequest);
        }
        return open_recorded_reset_stage(
            local_guard,
            local_appdata,
            target,
            quarantine,
            owner,
            record,
            on_event,
        );
    }

    let root = WindowsFileHandle::open_for_rename(target)?;
    if root.identity != owner.root_identity
        || root.is_reparse()
        || !root.is_directory()
        || quarantine.exists()
    {
        return Err(DiagnosticError::ResetTargetConflict);
    }
    local_guard.revalidate()?;

    for _ in 0..32 {
        let staging_name = new_reset_staging_name(nonce)?;
        let child = target.join(&staging_name);
        let staging = local_appdata.join(&staging_name);
        if child.exists() || staging.exists() {
            continue;
        }

        on_event(ResetEvent::BeforeStageDirectoryCreate)?;
        match fs::create_dir(&child) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(classify_reset_io(error)),
        }
        on_event(ResetEvent::AfterStageDirectoryCreate)?;
        let staged = WindowsFileHandle::open_for_rename(&child)?;
        verify_empty_directory_handle(&child, &staged, staged.identity)?;
        let record = ResetStageRecord {
            nonce: nonce.to_owned(),
            staging_name,
            staging_identity: staged.identity,
        };
        write_reset_state_with_events(
            local_guard,
            local_appdata,
            stage_path,
            &record,
            nonce,
            ResetStateStep::Staged,
            on_event,
        )?;
        local_guard.revalidate()?;
        on_event(ResetEvent::BeforeRename(ResetRenameStep::Staging))?;
        staged.rename_to(&staging).map_err(classify_reset_error)?;
        on_event(ResetEvent::AfterRename(ResetRenameStep::Staging))?;
        verify_empty_directory_handle(&staging, &staged, record.staging_identity)?;
        drop(root);
        return Ok(PreparedResetStage {
            record,
            handle: staged,
            at_target: false,
        });
    }
    Err(DiagnosticError::InvalidResetRequest)
}

#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
fn open_recorded_reset_stage<F>(
    local_guard: &ResetRootGuard,
    local_appdata: &Path,
    target: &Path,
    quarantine: &Path,
    owner: &ResetOwnershipRecord,
    record: ResetStageRecord,
    on_event: &mut F,
) -> Result<PreparedResetStage, DiagnosticError>
where
    F: FnMut(ResetEvent) -> Result<(), DiagnosticError>,
{
    let staging = local_appdata.join(&record.staging_name);
    if staging.exists() {
        let staged = WindowsFileHandle::open_for_rename(&staging)?;
        verify_empty_directory_handle(&staging, &staged, record.staging_identity)?;
        return Ok(PreparedResetStage {
            record,
            handle: staged,
            at_target: false,
        });
    }

    if let Ok(created) = WindowsFileHandle::open_for_rename(target) {
        if created.identity == record.staging_identity {
            verify_empty_directory_handle(target, &created, record.staging_identity)?;
            return Ok(PreparedResetStage {
                record,
                handle: created,
                at_target: true,
            });
        }
    }

    let parent = if target.exists() {
        let root = WindowsFileHandle::open_pinned(target)?;
        if root.identity != owner.root_identity || root.is_reparse() || !root.is_directory() {
            return Err(DiagnosticError::ResetTargetConflict);
        }
        target
    } else if quarantine.exists() {
        let root = WindowsFileHandle::open_pinned(quarantine)?;
        if root.identity != owner.root_identity || root.is_reparse() || !root.is_directory() {
            return Err(DiagnosticError::ResetTargetConflict);
        }
        quarantine
    } else {
        return Err(DiagnosticError::InvalidResetRequest);
    };
    let child = parent.join(&record.staging_name);
    let staged = WindowsFileHandle::open_for_rename(&child)?;
    verify_empty_directory_handle(&child, &staged, record.staging_identity)?;
    local_guard.revalidate()?;
    on_event(ResetEvent::BeforeRename(ResetRenameStep::Staging))?;
    staged.rename_to(&staging).map_err(classify_reset_error)?;
    on_event(ResetEvent::AfterRename(ResetRenameStep::Staging))?;
    verify_empty_directory_handle(&staging, &staged, record.staging_identity)?;
    Ok(PreparedResetStage {
        record,
        handle: staged,
        at_target: false,
    })
}

#[cfg(windows)]
fn read_authenticated_reset_request(
    local_guard: &ResetRootGuard,
    local_appdata: &Path,
    request_path: &Path,
    commit_path: &Path,
    nonce: &str,
) -> Result<ResetRequest, DiagnosticError> {
    let completion_path = reset_completion_path(local_appdata, nonce)?;
    let startup_path = reset_startup_path(local_appdata, nonce)?;
    let request = if request_path.exists() {
        read_reset_state::<ResetRequest>(local_guard, local_appdata, request_path)?
    } else if commit_path.exists() {
        read_reset_state::<ResetCommitRecord>(local_guard, local_appdata, commit_path)?.request
    } else if completion_path.exists() {
        read_reset_state::<ResetCompletionRecord>(local_guard, local_appdata, &completion_path)?
            .commit
            .request
    } else if startup_path.exists() {
        read_reset_state::<ResetStartupRecord>(local_guard, local_appdata, &startup_path)?
            .completion
            .commit
            .request
    } else {
        return Err(DiagnosticError::InvalidResetRequest);
    };
    if request.nonce != nonce {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    Ok(request)
}

#[cfg(windows)]
fn read_terminal_completion(
    local_guard: &ResetRootGuard,
    local_appdata: &Path,
    completion_path: &Path,
    startup_path: &Path,
) -> Result<ResetCompletionRecord, DiagnosticError> {
    if completion_path.exists() {
        read_reset_state(local_guard, local_appdata, completion_path)
    } else if startup_path.exists() {
        Ok(
            read_reset_state::<ResetStartupRecord>(local_guard, local_appdata, startup_path)?
                .completion,
        )
    } else {
        Err(DiagnosticError::InvalidResetRequest)
    }
}

#[cfg(windows)]
fn validate_commit(
    commit: &ResetCommitRecord,
    request: &ResetRequest,
    nonce: &str,
) -> Result<(), DiagnosticError> {
    if &commit.request != request
        || commit.request.nonce != nonce
        || !valid_reset_staging_name(nonce, &commit.staging_name)
    {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    Ok(())
}

#[cfg(windows)]
fn validate_completion(
    completion: &ResetCompletionRecord,
    request: &ResetRequest,
    nonce: &str,
) -> Result<(), DiagnosticError> {
    validate_commit(&completion.commit, request, nonce)
}

#[cfg(windows)]
fn validate_startup(
    startup: &ResetStartupRecord,
    completion: &ResetCompletionRecord,
    nonce: &str,
) -> Result<(), DiagnosticError> {
    if &startup.completion != completion
        || startup.child_pid == 0
        || startup.child_created == 0
        || startup.completion.commit.request.nonce != nonce
    {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    validate_completion(completion, &completion.commit.request, nonce)
}

#[cfg(windows)]
fn validate_terminal_transaction_markers(
    local_guard: &ResetRootGuard,
    local_appdata: &Path,
    nonce: &str,
    completion: &ResetCompletionRecord,
) -> Result<(), DiagnosticError> {
    validate_completion(completion, &completion.commit.request, nonce)?;
    let request_path = reset_request_path(local_appdata, nonce)?;
    if request_path.exists()
        && read_reset_state::<ResetRequest>(local_guard, local_appdata, &request_path)?
            != completion.commit.request
    {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    let commit_path = reset_commit_path(local_appdata, nonce)?;
    if commit_path.exists()
        && read_reset_state::<ResetCommitRecord>(local_guard, local_appdata, &commit_path)?
            != completion.commit
    {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    let parent_exited_path = reset_parent_exited_path(local_appdata, nonce)?;
    if parent_exited_path.exists()
        && read_reset_state::<ResetRequest>(local_guard, local_appdata, &parent_exited_path)?
            != completion.commit.request
    {
        return Err(DiagnosticError::InvalidResetRequest);
    }
    let stage_path = reset_stage_path(local_appdata, nonce)?;
    if stage_path.exists() {
        let stage = read_reset_state::<ResetStageRecord>(local_guard, local_appdata, &stage_path)?;
        if stage.nonce != nonce
            || stage.staging_name != completion.commit.staging_name
            || stage.staging_identity != completion.commit.target_identity
        {
            return Err(DiagnosticError::InvalidResetRequest);
        }
    }

    let expected_quarantine = reset_quarantine_name(nonce)?;
    let owner_path = reset_owner_path(local_appdata, nonce)?;
    let owner = if owner_path.exists() {
        let owner =
            read_reset_state::<ResetOwnershipRecord>(local_guard, local_appdata, &owner_path)?;
        if owner.nonce != nonce || owner.quarantine_name != expected_quarantine {
            return Err(DiagnosticError::InvalidResetRequest);
        }
        Some(owner)
    } else {
        None
    };
    let mut phase_owner: Option<ResetOwnershipRecord> = None;
    for path in [
        reset_deleting_path(local_appdata, nonce)?,
        reset_cleaned_path(local_appdata, nonce)?,
    ] {
        if !path.exists() {
            continue;
        }
        let record = read_reset_state::<ResetOwnershipRecord>(local_guard, local_appdata, &path)?;
        if record.nonce != nonce
            || record.quarantine_name != expected_quarantine
            || owner.as_ref().is_some_and(|owner| owner != &record)
            || phase_owner
                .as_ref()
                .is_some_and(|previous| previous != &record)
        {
            return Err(DiagnosticError::InvalidResetRequest);
        }
        phase_owner = Some(record);
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
fn open_verified_directory_identity(
    path: &Path,
    expected_identity: WindowsFileIdentity,
) -> Result<WindowsFileHandle, DiagnosticError> {
    let pinned = WindowsFileHandle::open_pinned(path)?;
    if pinned.identity != expected_identity || pinned.is_reparse() || !pinned.is_directory() {
        return Err(DiagnosticError::ResetTargetConflict);
    }
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
        local_guard.sync_directory_supported()?;
        local_guard.revalidate()?;
        before_cleanup(ResetEvent::AfterStateCleanup(step))?;
    }
    Ok(())
}

#[cfg(windows)]
fn open_owned_quarantine<F>(
    local_guard: &ResetRootGuard,
    target: &Path,
    quarantine: &Path,
    owner: &ResetOwnershipRecord,
    on_event: &mut F,
) -> Result<WindowsFileHandle, DiagnosticError>
where
    F: FnMut(ResetEvent) -> Result<(), DiagnosticError>,
{
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
    on_event(ResetEvent::BeforeRename(ResetRenameStep::Quarantine))?;
    root.rename_to(quarantine).map_err(classify_reset_error)?;
    on_event(ResetEvent::AfterRename(ResetRenameStep::Quarantine))?;
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

    fn sync_directory_supported(&self) -> Result<(), DiagnosticError> {
        use windows::Win32::Storage::FileSystem::FlushFileBuffers;

        match unsafe { FlushFileBuffers(self.handle) } {
            Ok(()) => Ok(()),
            Err(error) => {
                let code = error.code().0 as u32;
                let os_code = if code & 0xFFFF_0000 == 0x8007_0000 {
                    code & 0x0000_FFFF
                } else {
                    code
                };
                // Windows filesystems do not universally permit flushing directory
                // handles. The pinned identity still closes the substitution race.
                if matches!(os_code, 1 | 2 | 5 | 6 | 50 | 87) {
                    Ok(())
                } else {
                    Err(windows_error(error))
                }
            }
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
        Self::open(path, false, false)
    }

    fn open_for_rename(path: &Path) -> Result<Self, DiagnosticError> {
        Self::open(path, true, true)
    }

    fn open_for_identity(path: &Path) -> Result<Self, DiagnosticError> {
        Self::open(path, true, false)
    }

    fn open(path: &Path, share_delete: bool, delete_access: bool) -> Result<Self, DiagnosticError> {
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
        let access = if delete_access {
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
        rename_windows_handle_to(self.handle, destination)
    }
}

#[cfg(windows)]
fn rename_windows_handle_to(
    handle: windows::Win32::Foundation::HANDLE,
    destination: &Path,
) -> Result<(), DiagnosticError> {
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
            handle,
            FileRenameInfo,
            info.cast(),
            u32::try_from(total).map_err(|_| DiagnosticError::InvalidResetRequest)?,
        )
    }
    .map_err(windows_error)
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
        let mut no_event = |_| Ok(());
        create_reset_state(
            &local_guard,
            &local,
            &request_path,
            &ResetRequest {
                nonce: nonce.into(),
                parent_pid: 123,
                parent_created: 456,
            },
            nonce,
            ResetStateStep::Request,
            &mut no_event,
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
        let mut no_event = |_| Ok(());
        create_reset_state(
            &local_guard,
            &local,
            &request_path,
            &ResetRequest {
                nonce: nonce.into(),
                parent_pid: 123,
                parent_created: 456,
            },
            nonce,
            ResetStateStep::Request,
            &mut no_event,
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
    fn worker_recovers_when_stage_creation_precedes_its_identity_record() {
        let temp = tempfile::tempdir().unwrap();
        let local = temp.path().join("Local");
        let app_data = local.join(APP_DATA_DIRECTORY);
        fs::create_dir_all(&app_data).unwrap();
        fs::write(app_data.join("owned"), b"owned").unwrap();

        let nonce = "3123456789abcdef0123456789abcdef";
        let local_guard = ResetRootGuard::open(&local).unwrap();
        let request_path = reset_request_path(&local, nonce).unwrap();
        let request = ResetRequest {
            nonce: nonce.into(),
            parent_pid: 123,
            parent_created: 456,
        };
        let mut no_event = |_| Ok(());
        create_reset_state(
            &local_guard,
            &local,
            &request_path,
            &request,
            nonce,
            ResetStateStep::Request,
            &mut no_event,
        )
        .unwrap();
        let mut wait_for_parent = |_, _| Ok(());
        let mut fail_after_create = true;
        let mut on_event = |event| {
            if fail_after_create && event == ResetEvent::AfterStageDirectoryCreate {
                fail_after_create = false;
                return Err(DiagnosticError::Io(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "injected stage create-to-record crash",
                )));
            }
            Ok(())
        };
        assert_eq!(
            run_authenticated_reset_worker(
                &local,
                nonce,
                &request,
                1,
                Duration::ZERO,
                &mut wait_for_parent,
                &mut on_event,
            ),
            ResetWorkerOutcome::Failed
        );
        assert_eq!(fs::read(app_data.join("owned")).unwrap(), b"owned");
        assert!(!local.join(reset_quarantine_name(nonce).unwrap()).exists());

        let mut no_fault = |_| Ok(());
        assert_eq!(
            run_authenticated_reset_worker(
                &local,
                nonce,
                &request,
                1,
                Duration::ZERO,
                &mut wait_for_parent,
                &mut no_fault,
            ),
            ResetWorkerOutcome::Success
        );
        assert!(app_data.is_dir());
        assert_eq!(fs::read_dir(&app_data).unwrap().count(), 0);
    }

    #[test]
    fn durable_state_write_faults_never_publish_partial_json() {
        let fault_points = [
            ResetStateWritePoint::AfterTempCreate,
            ResetStateWritePoint::AfterPartialWrite,
            ResetStateWritePoint::AfterTempSync,
            ResetStateWritePoint::BeforeRename,
            ResetStateWritePoint::AfterRename,
        ];
        let state_steps = [
            ResetStateStep::Request,
            ResetStateStep::ParentExited,
            ResetStateStep::Owner,
            ResetStateStep::Staged,
            ResetStateStep::Deleting,
            ResetStateStep::Deleted,
            ResetStateStep::Committed,
            ResetStateStep::Completed,
            ResetStateStep::Startup,
        ];

        for (case, (step, fault_point)) in state_steps
            .into_iter()
            .flat_map(|step| fault_points.into_iter().map(move |point| (step, point)))
            .enumerate()
        {
            let temp = tempfile::tempdir().unwrap();
            let local = temp.path().join("Local");
            fs::create_dir_all(&local).unwrap();
            let nonce = format!("{case:032x}");
            let final_path = local.join(format!("state-{case}.json"));
            let request = ResetRequest {
                nonce: nonce.clone(),
                parent_pid: 123,
                parent_created: 456,
            };
            let local_guard = ResetRootGuard::open(&local).unwrap();
            let mut faulted = false;
            let mut inject_once = |event| {
                if !faulted
                    && event
                        == (ResetEvent::StateWritePoint {
                            step,
                            point: fault_point,
                        })
                {
                    faulted = true;
                    return Err(DiagnosticError::Io(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "injected durable state write crash",
                    )));
                }
                Ok(())
            };

            assert!(write_reset_state_with_events(
                &local_guard,
                &local,
                &final_path,
                &request,
                &nonce,
                step,
                &mut inject_once,
            )
            .is_err());
            assert!(
                faulted,
                "write failed before injected point {step:?}/{fault_point:?}"
            );
            if final_path.exists() {
                assert_eq!(
                    read_reset_state::<ResetRequest>(&local_guard, &local, &final_path).unwrap(),
                    request,
                    "published marker must always be complete after {step:?}/{fault_point:?}"
                );
            }

            if !final_path.exists() {
                let mut no_fault = |_| Ok(());
                write_reset_state_with_events(
                    &local_guard,
                    &local,
                    &final_path,
                    &request,
                    &nonce,
                    step,
                    &mut no_fault,
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "retry write failed for case {case} at {step:?}/{fault_point:?}: {error:?}"
                    )
                });
            }
            assert_eq!(
                read_reset_state::<ResetRequest>(&local_guard, &local, &final_path).unwrap(),
                request
            );
        }
    }

    #[test]
    fn reset_recovery_never_deletes_a_substituted_nonce_shaped_temp_file() {
        let temp = tempfile::tempdir().unwrap();
        let local = temp.path().join("Local");
        let app_data = local.join(APP_DATA_DIRECTORY);
        fs::create_dir_all(&app_data).unwrap();
        fs::write(app_data.join("owned"), b"owned").unwrap();

        let nonce = "00000000000000000000000000000500";
        let substituted = local.join(format!(
            ".{APP_DATA_DIRECTORY}-reset-state-temp-{nonce}-owner-\
             0123456789abcdef0123456789abcdef.tmp"
        ));
        fs::write(&substituted, b"unowned-victim").unwrap();

        let request = ResetRequest {
            nonce: nonce.to_owned(),
            parent_pid: 123,
            parent_created: 456,
        };
        let local_guard = ResetRootGuard::open(&local).unwrap();
        let request_path = reset_request_path(&local, nonce).unwrap();
        let mut no_event = |_| Ok(());
        create_reset_state(
            &local_guard,
            &local,
            &request_path,
            &request,
            nonce,
            ResetStateStep::Request,
            &mut no_event,
        )
        .unwrap();

        let mut wait_for_parent = |_: u32, _: u64| Ok(());
        assert_eq!(
            run_authenticated_reset_worker(
                &local,
                nonce,
                &request,
                3,
                Duration::ZERO,
                &mut wait_for_parent,
                &mut no_event,
            ),
            ResetWorkerOutcome::Success
        );
        assert_eq!(fs::read(&substituted).unwrap(), b"unowned-victim");
    }

    #[test]
    fn malformed_final_markers_fail_closed_and_are_never_cleaned_as_temps() {
        let temp = tempfile::tempdir().unwrap();
        let local = temp.path().join("Local");
        let app_data = local.join(APP_DATA_DIRECTORY);
        let victim = local.join("victim");
        fs::create_dir_all(&app_data).unwrap();
        fs::create_dir_all(&victim).unwrap();
        fs::write(app_data.join("owned"), b"owned").unwrap();
        fs::write(victim.join("must-survive"), b"victim").unwrap();
        let nonce = "00000000000000000000000000000400";
        let request = ResetRequest {
            nonce: nonce.into(),
            parent_pid: 123,
            parent_created: 456,
        };
        let local_guard = ResetRootGuard::open(&local).unwrap();
        let mut no_event = |_| Ok(());
        create_reset_state(
            &local_guard,
            &local,
            &reset_request_path(&local, nonce).unwrap(),
            &request,
            nonce,
            ResetStateStep::Request,
            &mut no_event,
        )
        .unwrap();
        let mut wait_for_parent = |_, _| Ok(());
        let mut stopped = false;
        let mut stop_after_completion = |event| {
            if !stopped && event == ResetEvent::AfterStateWrite(ResetStateStep::Completed) {
                stopped = true;
                return Err(DiagnosticError::Io(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "stop with all terminal cleanup markers present",
                )));
            }
            Ok(())
        };
        assert_eq!(
            run_authenticated_reset_worker(
                &local,
                nonce,
                &request,
                1,
                Duration::ZERO,
                &mut wait_for_parent,
                &mut stop_after_completion,
            ),
            ResetWorkerOutcome::Failed
        );
        assert!(stopped);
        let owner_path = reset_owner_path(&local, nonce).unwrap();
        fs::remove_file(&owner_path).unwrap();
        fs::write(&owner_path, b"{").unwrap();

        assert_eq!(
            run_authenticated_reset_worker(
                &local,
                nonce,
                &request,
                1,
                Duration::ZERO,
                &mut wait_for_parent,
                &mut no_event,
            ),
            ResetWorkerOutcome::Failed
        );
        assert_eq!(fs::read(&owner_path).unwrap(), b"{");
        assert!(reset_completion_path(&local, nonce).unwrap().is_file());
        assert_eq!(fs::read(victim.join("must-survive")).unwrap(), b"victim");
    }

    #[test]
    fn restart_receipt_survives_spawn_faults_and_child_cleans_it_after_startup() {
        let fault_points = [
            ResetEvent::BeforeRestartSpawn,
            ResetEvent::AfterRestartSpawn,
        ];

        for (case, fault_point) in fault_points.into_iter().enumerate() {
            let temp = tempfile::tempdir().unwrap();
            let local = temp.path().join("Local");
            let app_data = local.join(APP_DATA_DIRECTORY);
            fs::create_dir_all(&app_data).unwrap();
            fs::write(app_data.join("owned"), format!("owned-{case}")).unwrap();
            let nonce = format!("{:032x}", case + 500);
            let request = ResetRequest {
                nonce: nonce.clone(),
                parent_pid: 123,
                parent_created: 456,
            };
            let local_guard = ResetRootGuard::open(&local).unwrap();
            let mut no_fault = |_| Ok(());
            create_reset_state(
                &local_guard,
                &local,
                &reset_request_path(&local, &nonce).unwrap(),
                &request,
                &nonce,
                ResetStateStep::Request,
                &mut no_fault,
            )
            .unwrap();

            let mut wait_for_parent = |_, _| Ok(());
            let mut spawn_count = 0;
            let mut spawn = |_: &str| {
                spawn_count += 1;
                Ok(())
            };
            let mut process_alive = |_, _| Ok(false);
            let mut faulted = false;
            let mut inject_once = |event| {
                if !faulted && event == fault_point {
                    faulted = true;
                    return Err(DiagnosticError::Io(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "injected restart handoff crash",
                    )));
                }
                Ok(())
            };
            assert_eq!(
                run_authenticated_reset_worker_and_restart(
                    &local,
                    &nonce,
                    &request,
                    1,
                    Duration::ZERO,
                    &mut wait_for_parent,
                    &mut process_alive,
                    &mut spawn,
                    &mut inject_once,
                ),
                ResetWorkerOutcome::Failed
            );
            assert!(reset_completion_path(&local, &nonce).unwrap().is_file());

            let mut no_fault = |_| Ok(());
            assert_eq!(
                run_authenticated_reset_worker_and_restart(
                    &local,
                    &nonce,
                    &request,
                    1,
                    Duration::ZERO,
                    &mut wait_for_parent,
                    &mut process_alive,
                    &mut spawn,
                    &mut no_fault,
                ),
                ResetWorkerOutcome::Success
            );
            assert!(spawn_count >= 1);

            let startup = prepare_reset_completion(
                &local,
                &nonce,
                789,
                987,
                &mut process_alive,
                &mut no_fault,
            )
            .unwrap();
            fs::write(app_data.join("everyfile.db"), b"initialized-empty-db").unwrap();
            complete_reset_startup(startup, &mut no_fault).unwrap();
            assert!(!reset_artifacts(&local));
        }
    }

    #[test]
    fn helper_recovers_after_child_starts_before_spawn_returns() {
        use std::cell::RefCell;

        let temp = tempfile::tempdir().unwrap();
        let local = temp.path().join("Local");
        let app_data = local.join(APP_DATA_DIRECTORY);
        fs::create_dir_all(&app_data).unwrap();
        fs::write(app_data.join("owned"), b"owned").unwrap();
        let nonce = "00000000000000000000000000000258";
        let request = ResetRequest {
            nonce: nonce.into(),
            parent_pid: 123,
            parent_created: 456,
        };
        let local_guard = ResetRootGuard::open(&local).unwrap();
        let mut no_event = |_| Ok(());
        create_reset_state(
            &local_guard,
            &local,
            &reset_request_path(&local, nonce).unwrap(),
            &request,
            nonce,
            ResetStateStep::Request,
            &mut no_event,
        )
        .unwrap();

        let startup = RefCell::new(None);
        let spawn_count = std::cell::Cell::new(0);
        let mut spawn_and_claim = |_: &str| {
            spawn_count.set(spawn_count.get() + 1);
            let mut nobody_alive = |_, _| Ok(false);
            let mut no_event = |_| Ok(());
            startup.replace(Some(prepare_reset_completion(
                &local,
                nonce,
                789,
                987,
                &mut nobody_alive,
                &mut no_event,
            )?));
            Ok(())
        };
        let mut wait_for_parent = |_, _| Ok(());
        let mut nobody_alive = |_, _| Ok(false);
        let mut stop_after_spawn = |event| {
            if event == ResetEvent::AfterRestartSpawn {
                Err(DiagnosticError::Io(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "helper crashed after child claimed startup",
                )))
            } else {
                Ok(())
            }
        };
        assert_eq!(
            run_authenticated_reset_worker_and_restart(
                &local,
                nonce,
                &request,
                1,
                Duration::ZERO,
                &mut wait_for_parent,
                &mut nobody_alive,
                &mut spawn_and_claim,
                &mut stop_after_spawn,
            ),
            ResetWorkerOutcome::Failed
        );
        assert_eq!(spawn_count.get(), 1);

        let mut child_alive = |pid, created| Ok(pid == 789 && created == 987);
        let mut unexpected_spawn = |_: &str| -> Result<(), DiagnosticError> {
            panic!("a live authenticated startup must not be spawned twice")
        };
        let mut no_event = |_| Ok(());
        assert_eq!(
            run_authenticated_reset_worker_and_restart(
                &local,
                nonce,
                &request,
                1,
                Duration::ZERO,
                &mut wait_for_parent,
                &mut child_alive,
                &mut unexpected_spawn,
                &mut no_event,
            ),
            ResetWorkerOutcome::Success
        );
        fs::write(app_data.join("everyfile.db"), b"initialized-empty-db").unwrap();
        complete_reset_startup(startup.take().unwrap(), &mut no_event).unwrap();
        assert!(!reset_artifacts(&local));
    }

    #[test]
    fn helper_replaces_a_stale_failed_child_claim_without_auth_gap() {
        let temp = tempfile::tempdir().unwrap();
        let local = temp.path().join("Local");
        let app_data = local.join(APP_DATA_DIRECTORY);
        fs::create_dir_all(&app_data).unwrap();
        fs::write(app_data.join("owned"), b"owned").unwrap();
        let nonce = "00000000000000000000000000000259";
        let request = ResetRequest {
            nonce: nonce.into(),
            parent_pid: 123,
            parent_created: 456,
        };
        let local_guard = ResetRootGuard::open(&local).unwrap();
        let mut no_event = |_| Ok(());
        create_reset_state(
            &local_guard,
            &local,
            &reset_request_path(&local, nonce).unwrap(),
            &request,
            nonce,
            ResetStateStep::Request,
            &mut no_event,
        )
        .unwrap();
        let mut wait_for_parent = |_, _| Ok(());
        assert_eq!(
            run_authenticated_reset_worker(
                &local,
                nonce,
                &request,
                1,
                Duration::ZERO,
                &mut wait_for_parent,
                &mut no_event,
            ),
            ResetWorkerOutcome::Success
        );
        let mut nobody_alive = |_, _| Ok(false);
        let stale =
            prepare_reset_completion(&local, nonce, 789, 987, &mut nobody_alive, &mut no_event)
                .unwrap();
        drop(stale);

        let mut spawn_count = 0;
        let mut spawn = |_: &str| {
            spawn_count += 1;
            Ok(())
        };
        assert_eq!(
            run_authenticated_reset_worker_and_restart(
                &local,
                nonce,
                &request,
                1,
                Duration::ZERO,
                &mut wait_for_parent,
                &mut nobody_alive,
                &mut spawn,
                &mut no_event,
            ),
            ResetWorkerOutcome::Success
        );
        assert_eq!(spawn_count, 1);
        assert!(reset_completion_path(&local, nonce).unwrap().is_file());
        assert!(!reset_startup_path(&local, nonce).unwrap().exists());
    }

    #[test]
    fn startup_receipt_cleanup_is_resumable_until_the_final_success_boundary() {
        let fault_points = [
            ResetEvent::BeforeStateCleanup(ResetCleanupStep::Completed),
            ResetEvent::AfterStateCleanup(ResetCleanupStep::Completed),
            ResetEvent::BeforeStateCleanup(ResetCleanupStep::Startup),
            ResetEvent::AfterStateCleanup(ResetCleanupStep::Startup),
        ];

        for (case, fault_point) in fault_points.into_iter().enumerate() {
            let temp = tempfile::tempdir().unwrap();
            let local = temp.path().join("Local");
            let app_data = local.join(APP_DATA_DIRECTORY);
            fs::create_dir_all(&app_data).unwrap();
            fs::write(app_data.join("owned"), format!("owned-{case}")).unwrap();
            let nonce = format!("{:032x}", case + 700);
            let request = ResetRequest {
                nonce: nonce.clone(),
                parent_pid: 123,
                parent_created: 456,
            };
            let local_guard = ResetRootGuard::open(&local).unwrap();
            let mut no_event = |_| Ok(());
            create_reset_state(
                &local_guard,
                &local,
                &reset_request_path(&local, &nonce).unwrap(),
                &request,
                &nonce,
                ResetStateStep::Request,
                &mut no_event,
            )
            .unwrap();
            let mut wait_for_parent = |_, _| Ok(());
            assert_eq!(
                run_authenticated_reset_worker(
                    &local,
                    &nonce,
                    &request,
                    1,
                    Duration::ZERO,
                    &mut wait_for_parent,
                    &mut no_event,
                ),
                ResetWorkerOutcome::Success
            );
            let mut nobody_alive = |_, _| Ok(false);
            let startup = prepare_reset_completion(
                &local,
                &nonce,
                789,
                987,
                &mut nobody_alive,
                &mut no_event,
            )
            .unwrap();
            fs::write(app_data.join("everyfile.db"), b"initialized-empty-db").unwrap();

            let mut faulted = false;
            let mut inject_once = |event| {
                if !faulted && event == fault_point {
                    faulted = true;
                    return Err(DiagnosticError::Io(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "injected startup receipt cleanup crash",
                    )));
                }
                Ok(())
            };
            assert!(complete_reset_startup(startup, &mut inject_once).is_err());
            assert!(faulted);

            if reset_artifacts(&local) {
                let resumed = prepare_reset_completion(
                    &local,
                    &nonce,
                    789,
                    987,
                    &mut nobody_alive,
                    &mut no_event,
                )
                .unwrap();
                complete_reset_startup(resumed, &mut no_event).unwrap();
            }
            assert!(
                !reset_artifacts(&local),
                "startup cleanup did not converge after {fault_point:?}"
            );
        }
    }

    #[test]
    fn production_worker_converges_across_every_reset_phase_fault() {
        let mut fault_points = vec![
            ResetEvent::BeforeStateWrite(ResetStateStep::ParentExited),
            ResetEvent::AfterStateWrite(ResetStateStep::ParentExited),
            ResetEvent::BeforeStateWrite(ResetStateStep::Owner),
            ResetEvent::AfterStateWrite(ResetStateStep::Owner),
            ResetEvent::BeforeStageDirectoryCreate,
            ResetEvent::AfterStageDirectoryCreate,
            ResetEvent::BeforeStateWrite(ResetStateStep::Staged),
            ResetEvent::AfterStateWrite(ResetStateStep::Staged),
            ResetEvent::BeforeRename(ResetRenameStep::Staging),
            ResetEvent::AfterRename(ResetRenameStep::Staging),
            ResetEvent::BeforeRename(ResetRenameStep::Quarantine),
            ResetEvent::AfterRename(ResetRenameStep::Quarantine),
            ResetEvent::BeforeStateWrite(ResetStateStep::Deleting),
            ResetEvent::AfterStateWrite(ResetStateStep::Deleting),
            ResetEvent::BeforeQuarantineCleanup { attempt: 0 },
            ResetEvent::AfterQuarantineDeletion,
            ResetEvent::BeforeStateWrite(ResetStateStep::Deleted),
            ResetEvent::AfterStateWrite(ResetStateStep::Deleted),
            ResetEvent::BeforeRename(ResetRenameStep::Target),
            ResetEvent::AfterRename(ResetRenameStep::Target),
            ResetEvent::BeforeStateWrite(ResetStateStep::Committed),
            ResetEvent::AfterStateWrite(ResetStateStep::Committed),
            ResetEvent::BeforeStateCleanup(ResetCleanupStep::Deleting),
            ResetEvent::AfterStateCleanup(ResetCleanupStep::Deleting),
            ResetEvent::BeforeStateCleanup(ResetCleanupStep::Deleted),
            ResetEvent::AfterStateCleanup(ResetCleanupStep::Deleted),
            ResetEvent::BeforeStateCleanup(ResetCleanupStep::Staged),
            ResetEvent::AfterStateCleanup(ResetCleanupStep::Staged),
            ResetEvent::BeforeStateCleanup(ResetCleanupStep::Owner),
            ResetEvent::AfterStateCleanup(ResetCleanupStep::Owner),
            ResetEvent::BeforeStateCleanup(ResetCleanupStep::ParentExited),
            ResetEvent::AfterStateCleanup(ResetCleanupStep::ParentExited),
            ResetEvent::BeforeStateCleanup(ResetCleanupStep::Request),
            ResetEvent::AfterStateCleanup(ResetCleanupStep::Request),
            ResetEvent::BeforeStateCleanup(ResetCleanupStep::Committed),
            ResetEvent::AfterStateCleanup(ResetCleanupStep::Committed),
        ];
        let write_points = [
            ResetStateWritePoint::AfterTempCreate,
            ResetStateWritePoint::AfterPartialWrite,
            ResetStateWritePoint::AfterTempSync,
            ResetStateWritePoint::BeforeRename,
            ResetStateWritePoint::AfterRename,
        ];
        for step in [
            ResetStateStep::ParentExited,
            ResetStateStep::Owner,
            ResetStateStep::Staged,
            ResetStateStep::Deleting,
            ResetStateStep::Deleted,
            ResetStateStep::Committed,
            ResetStateStep::Completed,
        ] {
            fault_points.extend(
                write_points
                    .into_iter()
                    .map(|point| ResetEvent::StateWritePoint { step, point }),
            );
        }
        let mut pre_destructive = vec![
            ResetEvent::BeforeStateWrite(ResetStateStep::ParentExited),
            ResetEvent::AfterStateWrite(ResetStateStep::ParentExited),
            ResetEvent::BeforeStateWrite(ResetStateStep::Owner),
            ResetEvent::AfterStateWrite(ResetStateStep::Owner),
            ResetEvent::BeforeStageDirectoryCreate,
            ResetEvent::AfterStageDirectoryCreate,
            ResetEvent::BeforeStateWrite(ResetStateStep::Staged),
            ResetEvent::AfterStateWrite(ResetStateStep::Staged),
            ResetEvent::BeforeRename(ResetRenameStep::Staging),
            ResetEvent::AfterRename(ResetRenameStep::Staging),
            ResetEvent::BeforeRename(ResetRenameStep::Quarantine),
        ];
        for step in [
            ResetStateStep::ParentExited,
            ResetStateStep::Owner,
            ResetStateStep::Staged,
        ] {
            pre_destructive.extend(
                write_points
                    .into_iter()
                    .map(|point| ResetEvent::StateWritePoint { step, point }),
            );
        }

        for (case, fault_point) in fault_points.into_iter().enumerate() {
            let temp = tempfile::tempdir().unwrap();
            let local = temp.path().join("Local");
            let app_data = local.join(APP_DATA_DIRECTORY);
            let victim = local.join("victim");
            fs::create_dir_all(&app_data).unwrap();
            fs::create_dir_all(&victim).unwrap();
            fs::write(app_data.join("owned"), format!("owned-{case}")).unwrap();
            fs::write(victim.join("must-survive"), b"victim").unwrap();

            let nonce = format!("{case:032x}");
            let request = ResetRequest {
                nonce: nonce.clone(),
                parent_pid: 123,
                parent_created: 456,
            };
            let local_guard = ResetRootGuard::open(&local).unwrap();
            let mut no_event = |_| Ok(());
            create_reset_state(
                &local_guard,
                &local,
                &reset_request_path(&local, &nonce).unwrap(),
                &request,
                &nonce,
                ResetStateStep::Request,
                &mut no_event,
            )
            .unwrap();
            let mut wait_for_parent = |_, _| Ok(());
            let mut faulted = false;
            let mut inject_once = |event| {
                if !faulted && event == fault_point {
                    faulted = true;
                    return Err(DiagnosticError::Io(io::Error::new(
                        io::ErrorKind::Interrupted,
                        format!("injected reset fault at {fault_point:?}"),
                    )));
                }
                Ok(())
            };
            assert_eq!(
                run_authenticated_reset_worker(
                    &local,
                    &nonce,
                    &request,
                    1,
                    Duration::ZERO,
                    &mut wait_for_parent,
                    &mut inject_once,
                ),
                ResetWorkerOutcome::Failed,
                "fault case {fault_point:?}"
            );
            assert!(faulted, "fault point was not reached: {fault_point:?}");
            if pre_destructive.contains(&fault_point) {
                assert_eq!(
                    fs::read(app_data.join("owned")).unwrap(),
                    format!("owned-{case}").as_bytes(),
                    "pre-destructive fault changed the exact original root: {fault_point:?}"
                );
                assert!(
                    !local.join(reset_quarantine_name(&nonce).unwrap()).exists(),
                    "pre-destructive fault quarantined the original root: {fault_point:?}"
                );
            }

            let mut no_fault = |_| Ok(());
            let mut successes = 0;
            for _ in 0..3 {
                let outcome = run_authenticated_reset_worker(
                    &local,
                    &nonce,
                    &request,
                    1,
                    Duration::ZERO,
                    &mut wait_for_parent,
                    &mut no_fault,
                );
                if outcome == ResetWorkerOutcome::Success {
                    successes += 1;
                    break;
                }
                assert_eq!(
                    outcome,
                    ResetWorkerOutcome::Failed,
                    "unexpected resume outcome after {fault_point:?}"
                );
            }
            assert_eq!(successes, 1, "did not converge after {fault_point:?}");
            assert!(app_data.is_dir());
            assert_eq!(
                fs::read_dir(&app_data).unwrap().count(),
                0,
                "recreated root was not empty after {fault_point:?}"
            );
            assert_eq!(
                fs::read(victim.join("must-survive")).unwrap(),
                b"victim",
                "outside victim changed after {fault_point:?}"
            );
            assert!(
                only_completion_receipt_remains(&local, &nonce),
                "non-terminal reset artifacts remained after {fault_point:?}"
            );
        }
    }

    #[test]
    fn production_preflight_uses_commit_after_parent_exit_receipt_cleanup() {
        use std::cell::Cell;

        let temp = tempfile::tempdir().unwrap();
        let local = temp.path().join("Local");
        let app_data = local.join(APP_DATA_DIRECTORY);
        let victim = local.join("victim");
        fs::create_dir_all(&app_data).unwrap();
        fs::create_dir_all(&victim).unwrap();
        fs::write(app_data.join("owned"), b"owned").unwrap();
        fs::write(victim.join("must-survive"), b"victim").unwrap();

        let nonce = "4123456789abcdef0123456789abcdef";
        let request = ResetRequest {
            nonce: nonce.into(),
            parent_pid: 123,
            parent_created: 456,
        };
        let local_guard = ResetRootGuard::open(&local).unwrap();
        let mut no_event = |_| Ok(());
        create_reset_state(
            &local_guard,
            &local,
            &reset_request_path(&local, nonce).unwrap(),
            &request,
            nonce,
            ResetStateStep::Request,
            &mut no_event,
        )
        .unwrap();

        let wait_calls = Cell::new(0);
        let mut wait_for_parent = |_, _| {
            wait_calls.set(wait_calls.get() + 1);
            if wait_calls.get() == 1 {
                Ok(())
            } else {
                Err(DiagnosticError::InvalidResetRequest)
            }
        };
        let mut interrupted = false;
        let mut remove_parent_receipt_then_stop = |event| {
            if !interrupted
                && event == ResetEvent::AfterStateCleanup(ResetCleanupStep::ParentExited)
            {
                interrupted = true;
                return Err(DiagnosticError::Io(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "injected interruption after parent receipt cleanup",
                )));
            }
            Ok(())
        };
        assert_eq!(
            run_authenticated_reset_worker(
                &local,
                nonce,
                &request,
                1,
                Duration::ZERO,
                &mut wait_for_parent,
                &mut remove_parent_receipt_then_stop,
            ),
            ResetWorkerOutcome::Failed
        );
        assert!(interrupted);
        assert_eq!(wait_calls.get(), 1);
        assert!(!reset_parent_exited_path(&local, nonce).unwrap().exists());
        assert!(reset_commit_path(&local, nonce).unwrap().is_file());

        let mut no_fault = |_| Ok(());
        assert_eq!(
            run_authenticated_reset_worker(
                &local,
                nonce,
                &request,
                1,
                Duration::ZERO,
                &mut wait_for_parent,
                &mut no_fault,
            ),
            ResetWorkerOutcome::Success
        );
        assert_eq!(
            wait_calls.get(),
            1,
            "a valid commit must bypass the inaccessible/reused old PID"
        );
        assert!(app_data.is_dir());
        assert_eq!(fs::read_dir(&app_data).unwrap().count(), 0);
        assert_eq!(fs::read(victim.join("must-survive")).unwrap(), b"victim");
        assert!(only_completion_receipt_remains(&local, nonce));
    }

    fn reset_artifacts(local_appdata: &Path) -> bool {
        fs::read_dir(local_appdata).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(&format!(".{APP_DATA_DIRECTORY}-reset-"))
        })
    }

    fn only_completion_receipt_remains(local_appdata: &Path, nonce: &str) -> bool {
        let completion = reset_completion_path(local_appdata, nonce).unwrap();
        let abandoned_temp_prefix = format!(".{APP_DATA_DIRECTORY}-reset-state-temp-{nonce}-");
        completion.is_file()
            && fs::read_dir(local_appdata)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(&format!(".{APP_DATA_DIRECTORY}-reset-"))
                })
                .all(|entry| {
                    entry.path() == completion
                        || entry
                            .file_name()
                            .to_string_lossy()
                            .starts_with(&abandoned_temp_prefix)
                })
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
        let mut no_event = |_| Ok(());
        create_reset_state(
            &local_guard,
            &local,
            &request_path,
            &request,
            nonce,
            ResetStateStep::Request,
            &mut no_event,
        )
        .unwrap();
        create_reset_state(
            &local_guard,
            &local,
            &reset_parent_exited_path(&local, nonce).unwrap(),
            &request,
            nonce,
            ResetStateStep::ParentExited,
            &mut no_event,
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
                        ResetEvent::AfterRename(ResetRenameStep::Target) if fail_after_rename => {
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
        assert!(only_completion_receipt_remains(&local, nonce));
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
