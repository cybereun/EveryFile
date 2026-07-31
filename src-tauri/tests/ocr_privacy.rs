use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use everyfile_lib::ocr::{OcrClient, OcrError, OcrErrorCode, OcrMode};
use tempfile::TempDir;

fn fixture_file() -> (TempDir, PathBuf) {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("scan.png");
    std::fs::write(&path, b"fixture").unwrap();
    (temp, path)
}

#[test]
fn local_network_guard_rejects_external_connections() {
    let host = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("sidecar")
        .join("ocr-host");
    let output = Command::new("python")
        .env("PYTHONPATH", host)
        .args([
            "-c",
            "import socket; from ocr_host.main import install_network_guard; \
             install_network_guard(); \
             s=socket.socket(); \
             \ntry: s.connect(('203.0.113.1', 9)); raise SystemExit(2)\n\
             except OSError: raise SystemExit(0)",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "OCR network guard allowed an external connection"
    );
}

#[test]
fn client_kills_a_timed_out_engine() {
    let (_temp, path) = fixture_file();
    let client = OcrClient::with_program(
        "python",
        [
            "-u",
            "-c",
            "import sys,time; sys.stdin.readline(); time.sleep(5)",
        ],
        Duration::from_millis(100),
    );
    let result = client.recognize(path, OcrMode::Text, 1024, Arc::new(AtomicBool::new(false)));
    assert!(matches!(result, Err(OcrError::Timeout)));
}

#[test]
fn client_reports_crash_without_leaking_process_details() {
    let (_temp, path) = fixture_file();
    let client = OcrClient::with_program(
        "python",
        [
            "-u",
            "-c",
            "import sys; sys.stdin.readline(); raise SystemExit(7)",
        ],
        Duration::from_secs(1),
    );
    let error = client
        .recognize(path, OcrMode::Text, 1024, Arc::new(AtomicBool::new(false)))
        .unwrap_err();
    assert!(matches!(error, OcrError::UnexpectedExit));
    assert!(!error.to_string().contains('7'));
}

#[test]
fn client_preserves_the_bounded_too_large_contract() {
    let (_temp, path) = fixture_file();
    let response = r#"import json,sys
request=json.loads(sys.stdin.readline())
print(json.dumps({"id":request["id"],"ok":False,"error":{"code":"TOO_LARGE","message":"too large"}}),flush=True)"#;
    let client = OcrClient::with_program("python", ["-u", "-c", response], Duration::from_secs(1));
    let error = client
        .recognize(path, OcrMode::Text, 1, Arc::new(AtomicBool::new(false)))
        .unwrap_err();
    assert!(matches!(
        error,
        OcrError::Protocol {
            code: OcrErrorCode::TooLarge,
            ..
        }
    ));
}
