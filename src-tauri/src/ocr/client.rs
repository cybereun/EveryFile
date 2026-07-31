use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::parsing::ParsedDocument;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrMode {
    Text,
    Math,
}

impl OcrMode {
    fn as_wire(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Math => "math",
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OcrErrorCode {
    InvalidRequest,
    FileUnavailable,
    Unsupported,
    TooLarge,
    ModelMissing,
    InvalidEngineResult,
    Internal,
}

#[derive(Debug, Error, Clone)]
pub enum OcrError {
    #[error("OCR sidecar could not be started")]
    Start,
    #[error("OCR sidecar I/O failed")]
    Io,
    #[error("OCR sidecar exited unexpectedly")]
    UnexpectedExit,
    #[error("OCR sidecar returned an invalid response")]
    InvalidResponse,
    #[error("OCR request timed out")]
    Timeout,
    #[error("OCR rejected the document: {code:?}")]
    Protocol { code: OcrErrorCode, message: String },
}

#[derive(Clone)]
pub struct OcrClient {
    program: PathBuf,
    arguments: Vec<OsString>,
    timeout: Duration,
    sequence: std::sync::Arc<AtomicU64>,
    process: std::sync::Arc<Mutex<Option<OcrProcess>>>,
}

struct OcrProcess {
    child: Child,
    stdin: ChildStdin,
    responses: Receiver<Result<WireResponse, OcrError>>,
    reader: Option<JoinHandle<()>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireRequest<'a> {
    id: &'a str,
    operation: &'static str,
    path: &'a str,
    mode: &'static str,
    max_bytes: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireDocument {
    plain_text: String,
    markdown: String,
    blocks: Vec<Value>,
    metadata: Value,
    warnings: Vec<Value>,
}

#[derive(Deserialize)]
struct WireError {
    code: OcrErrorCode,
    message: String,
}

#[derive(Deserialize)]
struct WireResponse {
    id: Option<String>,
    ok: bool,
    document: Option<WireDocument>,
    error: Option<WireError>,
}

impl OcrClient {
    pub fn new(executable: impl Into<PathBuf>, timeout: Duration) -> Self {
        Self::with_program(executable, std::iter::empty::<OsString>(), timeout)
    }

    pub fn with_program<I, S>(
        executable: impl Into<PathBuf>,
        arguments: I,
        timeout: Duration,
    ) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        Self {
            program: executable.into(),
            arguments: arguments
                .into_iter()
                .map(|argument| argument.as_ref().to_os_string())
                .collect(),
            timeout,
            sequence: std::sync::Arc::new(AtomicU64::new(1)),
            process: std::sync::Arc::new(Mutex::new(None)),
        }
    }

    pub fn recognize(
        &self,
        path: impl AsRef<Path>,
        mode: OcrMode,
        max_bytes: u64,
    ) -> Result<ParsedDocument, OcrError> {
        let id = format!("ocr-{}", self.sequence.fetch_add(1, Ordering::Relaxed));
        let path = path.as_ref().to_string_lossy().into_owned();
        for attempt in 0..=1 {
            match self.send_and_wait(&id, &path, mode, max_bytes) {
                Err(OcrError::UnexpectedExit) if attempt == 0 => continue,
                result => return result,
            }
        }
        Err(OcrError::UnexpectedExit)
    }

    pub fn terminate(&self) {
        if let Ok(mut slot) = self.process.lock() {
            stop_process(&mut slot);
        }
    }

    fn send_and_wait(
        &self,
        id: &str,
        path: &str,
        mode: OcrMode,
        max_bytes: u64,
    ) -> Result<ParsedDocument, OcrError> {
        let mut slot = self.process.lock().map_err(|_| OcrError::Io)?;
        if slot
            .as_mut()
            .and_then(|process| process.child.try_wait().ok())
            .flatten()
            .is_some()
        {
            stop_process(&mut slot);
        }
        if slot.is_none() {
            *slot = Some(self.start_process()?);
        }
        let process = slot.as_mut().ok_or(OcrError::Start)?;
        let request = WireRequest {
            id,
            operation: "ocr",
            path,
            mode: mode.as_wire(),
            max_bytes,
        };
        if serde_json::to_writer(&mut process.stdin, &request).is_err()
            || process.stdin.write_all(b"\n").is_err()
            || process.stdin.flush().is_err()
        {
            stop_process(&mut slot);
            return Err(OcrError::UnexpectedExit);
        }
        let response = match process.responses.recv_timeout(self.timeout) {
            Ok(response) => response?,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                stop_process(&mut slot);
                return Err(OcrError::Timeout);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                stop_process(&mut slot);
                return Err(OcrError::UnexpectedExit);
            }
        };
        if response.id.as_deref() != Some(id) {
            stop_process(&mut slot);
            return Err(OcrError::InvalidResponse);
        }
        match (response.ok, response.document, response.error) {
            (true, Some(document), None) => Ok(ParsedDocument {
                title: None,
                markdown: document.markdown,
                plain_text: document.plain_text,
                blocks: document.blocks,
                metadata: document.metadata,
                warnings: document.warnings,
            }),
            (false, None, Some(error)) => Err(OcrError::Protocol {
                code: error.code,
                message: error.message,
            }),
            _ => Err(OcrError::InvalidResponse),
        }
    }

    fn start_process(&self) -> Result<OcrProcess, OcrError> {
        let mut command = Command::new(&self.program);
        command
            .args(&self.arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        configure_hidden_window(&mut command);
        let mut child = command.spawn().map_err(|_| OcrError::Start)?;
        let stdin = child.stdin.take().ok_or(OcrError::Start)?;
        let stdout = child.stdout.take().ok_or(OcrError::Start)?;
        let (sender, responses) = mpsc::channel();
        let reader = std::thread::Builder::new()
            .name("everyfile-ocr-reader".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let response = line.map_err(|_| OcrError::Io).and_then(|line| {
                        serde_json::from_str::<WireResponse>(&line)
                            .map_err(|_| OcrError::InvalidResponse)
                    });
                    if sender.send(response).is_err() {
                        return;
                    }
                }
            })
            .map_err(|_| OcrError::Start)?;
        Ok(OcrProcess {
            child,
            stdin,
            responses,
            reader: Some(reader),
        })
    }
}

impl Drop for OcrClient {
    fn drop(&mut self) {
        if std::sync::Arc::strong_count(&self.process) == 1 {
            self.terminate();
        }
    }
}

fn stop_process(slot: &mut Option<OcrProcess>) {
    if let Some(mut process) = slot.take() {
        let _ = process.child.kill();
        let _ = process.child.wait();
        if let Some(reader) = process.reader.take() {
            let _ = reader.join();
        }
    }
}

#[cfg(windows)]
fn configure_hidden_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn configure_hidden_window(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::{OcrErrorCode, WireResponse};

    #[test]
    fn parses_a_local_ocr_success_contract() {
        let response: WireResponse = serde_json::from_str(
            r#"{"id":"ocr-1","ok":true,"document":{"plainText":"local","markdown":"local","blocks":[],"metadata":{"localOnly":true},"warnings":[]}}"#,
        )
        .unwrap();
        assert!(response.ok);
        assert_eq!(response.id.as_deref(), Some("ocr-1"));
        assert_eq!(response.document.unwrap().plain_text, "local");
    }

    #[test]
    fn parses_model_missing_without_exposing_paths() {
        let response: WireResponse = serde_json::from_str(
            r#"{"id":"ocr-2","ok":false,"error":{"code":"MODEL_MISSING","message":"missing"}}"#,
        )
        .unwrap();
        assert_eq!(response.error.unwrap().code, OcrErrorCode::ModelMissing);
    }
}
