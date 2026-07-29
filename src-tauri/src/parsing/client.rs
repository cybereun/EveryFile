use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ParseErrorCode {
    InvalidRequest,
    Unsupported,
    Encrypted,
    Damaged,
    Timeout,
    TooLarge,
    ImageBasedPdf,
    Internal,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedDocument {
    pub title: Option<String>,
    pub markdown: String,
    pub plain_text: String,
    pub blocks: Vec<Value>,
    pub metadata: Value,
    pub warnings: Vec<Value>,
}

#[derive(Debug, Error, Clone)]
pub enum ParserError {
    #[error("parser sidecar could not be started")]
    Start,
    #[error("parser sidecar I/O failed")]
    Io,
    #[error("parser sidecar exited unexpectedly")]
    UnexpectedExit,
    #[error("parser sidecar returned an invalid response")]
    InvalidResponse,
    #[error("parser request timed out")]
    Timeout,
    #[error("parser rejected the document: {code:?}")]
    Protocol {
        code: ParseErrorCode,
        message: String,
    },
}

#[derive(Clone)]
pub struct ParserClient {
    inner: Arc<ClientInner>,
}

struct ClientInner {
    program: PathBuf,
    arguments: Vec<OsString>,
    timeout: Duration,
    sequence: AtomicU64,
    state: Mutex<ProcessState>,
    pending: Mutex<HashMap<String, PendingRequest>>,
}

struct ProcessState {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    reader: Option<JoinHandle<()>>,
    generation: u64,
}

struct PendingRequest {
    generation: u64,
    sender: Sender<Result<ParsedDocument, ParserError>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireRequest<'a> {
    id: &'a str,
    operation: &'static str,
    path: &'a str,
    options: WireOptions,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireOptions {
    max_bytes: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireResponse {
    id: Option<String>,
    ok: bool,
    document: Option<ParsedDocument>,
    error: Option<WireError>,
}

#[derive(Deserialize)]
struct WireError {
    code: ParseErrorCode,
    message: String,
}

impl ParserClient {
    pub fn new(executable: impl Into<PathBuf>, timeout: Duration) -> Self {
        Self::with_program(executable.into(), std::iter::empty::<OsString>(), timeout)
    }

    pub fn with_program<I, S>(program: impl Into<PathBuf>, arguments: I, timeout: Duration) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        Self {
            inner: Arc::new(ClientInner {
                program: program.into(),
                arguments: arguments
                    .into_iter()
                    .map(|argument| argument.as_ref().to_os_string())
                    .collect(),
                timeout,
                sequence: AtomicU64::new(1),
                state: Mutex::new(ProcessState {
                    child: None,
                    stdin: None,
                    reader: None,
                    generation: 0,
                }),
                pending: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn parse(
        &self,
        path: impl AsRef<Path>,
        max_bytes: u64,
    ) -> Result<ParsedDocument, ParserError> {
        let id = format!(
            "parse-{}",
            self.inner.sequence.fetch_add(1, Ordering::Relaxed)
        );
        let path = path.as_ref().to_string_lossy().into_owned();

        for attempt in 0..=1 {
            match self.inner.send_and_wait(&id, &path, max_bytes) {
                Err(ParserError::UnexpectedExit) if attempt == 0 => continue,
                result => return result,
            }
        }
        Err(ParserError::UnexpectedExit)
    }
}

impl ClientInner {
    fn send_and_wait(
        self: &Arc<Self>,
        id: &str,
        path: &str,
        max_bytes: u64,
    ) -> Result<ParsedDocument, ParserError> {
        let generation = self.ensure_process()?;
        let (sender, receiver) = mpsc::channel();
        lock(&self.pending).insert(id.to_owned(), PendingRequest { generation, sender });

        let request = WireRequest {
            id,
            operation: "parse",
            path,
            options: WireOptions { max_bytes },
        };
        let write_result = {
            let mut state = lock(&self.state);
            if state.generation != generation {
                Err(ParserError::UnexpectedExit)
            } else if let Some(stdin) = state.stdin.as_mut() {
                serde_json::to_writer(&mut *stdin, &request)
                    .map_err(|_| ParserError::Io)
                    .and_then(|_| stdin.write_all(b"\n").map_err(|_| ParserError::Io))
                    .and_then(|_| stdin.flush().map_err(|_| ParserError::Io))
            } else {
                Err(ParserError::UnexpectedExit)
            }
        };

        if write_result.is_err() {
            self.remove_pending(id, generation);
            self.terminate_generation(generation, ParserError::UnexpectedExit);
            return Err(ParserError::UnexpectedExit);
        }

        match receiver.recv_timeout(self.timeout) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.remove_pending(id, generation);
                self.terminate_generation(generation, ParserError::UnexpectedExit);
                Err(ParserError::Timeout)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(ParserError::UnexpectedExit),
        }
    }

    fn ensure_process(self: &Arc<Self>) -> Result<u64, ParserError> {
        loop {
            let stale_reader = {
                let mut state = lock(&self.state);
                if let Some(child) = state.child.as_mut() {
                    if matches!(child.try_wait(), Ok(None)) && state.stdin.is_some() {
                        return Ok(state.generation);
                    }
                    stop_process_locked(&mut state)
                } else if state.reader.is_some() {
                    stop_process_locked(&mut state)
                } else {
                    let mut command = Command::new(&self.program);
                    command
                        .args(&self.arguments)
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::null());
                    configure_hidden_window(&mut command);
                    let mut child = command.spawn().map_err(|_| ParserError::Start)?;
                    let stdin = child.stdin.take().ok_or(ParserError::Start)?;
                    let stdout = child.stdout.take().ok_or(ParserError::Start)?;
                    let generation = state.generation.wrapping_add(1);
                    let client = Arc::downgrade(self);
                    let reader = std::thread::Builder::new()
                        .name("everyfile-parser-reader".into())
                        .spawn(move || Self::read_responses(client, stdout, generation))
                        .map_err(|_| {
                            let _ = child.kill();
                            let _ = child.wait();
                            ParserError::Start
                        })?;
                    state.generation = generation;
                    state.stdin = Some(stdin);
                    state.child = Some(child);
                    state.reader = Some(reader);
                    return Ok(generation);
                }
            };
            join_reader(stale_reader);
        }
    }

    fn read_responses(client: Weak<Self>, stdout: impl std::io::Read, generation: u64) {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let line = match line {
                Ok(line) => line,
                Err(_) => break,
            };
            let Some(client) = client.upgrade() else {
                return;
            };
            let response: WireResponse = match serde_json::from_str(&line) {
                Ok(response) => response,
                Err(_) => {
                    client.fail_pending_generation(generation, ParserError::InvalidResponse);
                    continue;
                }
            };
            let Some(id) = response.id.as_deref() else {
                client.fail_pending_generation(generation, ParserError::InvalidResponse);
                continue;
            };
            let result = match (response.ok, response.document, response.error) {
                (true, Some(document), None) => Ok(document),
                (false, None, Some(error)) => Err(ParserError::Protocol {
                    code: error.code,
                    message: error.message,
                }),
                _ => Err(ParserError::InvalidResponse),
            };
            client.complete_pending(id, generation, result);
        }

        if let Some(client) = client.upgrade() {
            client.terminate_generation(generation, ParserError::UnexpectedExit);
        }
    }

    fn complete_pending(
        &self,
        id: &str,
        generation: u64,
        result: Result<ParsedDocument, ParserError>,
    ) {
        let mut pending = lock(&self.pending);
        if pending.get(id).map(|entry| entry.generation) != Some(generation) {
            return;
        }
        if let Some(entry) = pending.remove(id) {
            let _ = entry.sender.send(result);
        }
    }

    fn remove_pending(&self, id: &str, generation: u64) {
        let mut pending = lock(&self.pending);
        if pending.get(id).map(|entry| entry.generation) == Some(generation) {
            pending.remove(id);
        }
    }

    fn fail_pending_generation(&self, generation: u64, error: ParserError) {
        let mut pending = lock(&self.pending);
        let ids = pending
            .iter()
            .filter_map(|(id, entry)| (entry.generation == generation).then_some(id.clone()))
            .collect::<Vec<_>>();
        for id in ids {
            if let Some(entry) = pending.remove(&id) {
                let _ = entry.sender.send(Err(error.clone()));
            }
        }
    }

    fn terminate_generation(&self, generation: u64, pending_error: ParserError) {
        let reader = {
            let mut state = lock(&self.state);
            if state.generation == generation {
                stop_process_locked(&mut state)
            } else {
                None
            }
        };
        join_reader(reader);
        self.fail_pending_generation(generation, pending_error);
    }
}

impl Drop for ClientInner {
    fn drop(&mut self) {
        let reader = {
            let state = self
                .state
                .get_mut()
                .unwrap_or_else(|error| error.into_inner());
            stop_process_locked(state)
        };
        join_reader(reader);
    }
}

fn stop_process_locked(state: &mut ProcessState) -> Option<JoinHandle<()>> {
    state.stdin = None;
    if let Some(mut child) = state.child.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    state.reader.take()
}

fn join_reader(reader: Option<JoinHandle<()>>) {
    if let Some(reader) = reader {
        if reader.thread().id() != std::thread::current().id() {
            let _ = reader.join();
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

#[cfg(windows)]
fn configure_hidden_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn configure_hidden_window(_command: &mut Command) {}

pub const fn windows_creation_flags() -> u32 {
    CREATE_NO_WINDOW
}
