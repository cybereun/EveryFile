use std::fs;
use std::path::Path;
use std::time::Duration;

use everyfile_lib::parsing::{ParseErrorCode, ParserClient, ParserError};
use tempfile::TempDir;

fn write_fake_sidecar(directory: &Path, body: &str) -> std::path::PathBuf {
    let script = directory.join("fake-parser.mjs");
    fs::write(&script, body).unwrap();
    script
}

#[test]
fn matches_out_of_order_responses_by_request_id() {
    let temp = TempDir::new().unwrap();
    let script = write_fake_sidecar(
        temp.path(),
        r#"
import readline from "node:readline";
const pending = [];
readline.createInterface({ input: process.stdin }).on("line", line => {
  const request = JSON.parse(line);
  pending.push(request);
  if (pending.length === 2) {
    for (const item of pending.reverse()) {
      process.stdout.write(JSON.stringify({
        id: item.id,
        ok: true,
        document: {
          parserKind: "fake",
          title: null,
          markdown: item.path,
          plainText: item.path,
          blocks: [],
          metadata: {},
          warnings: []
        }
      }) + "\n");
    }
  }
});
"#,
    );
    let client = ParserClient::with_program("node", [script.as_os_str()], Duration::from_secs(2));

    let first_client = client.clone();
    let first = std::thread::spawn(move || first_client.parse("first.hwp", 1024));
    let second_client = client.clone();
    let second = std::thread::spawn(move || second_client.parse("second.pdf", 1024));

    assert_eq!(first.join().unwrap().unwrap().plain_text, "first.hwp");
    assert_eq!(second.join().unwrap().unwrap().plain_text, "second.pdf");
}

#[test]
fn returns_typed_protocol_errors_and_times_out() {
    let temp = TempDir::new().unwrap();
    let error_script = write_fake_sidecar(
        temp.path(),
        r#"
import readline from "node:readline";
readline.createInterface({ input: process.stdin }).on("line", line => {
  const request = JSON.parse(line);
  process.stdout.write(JSON.stringify({
    id: request.id,
    ok: false,
    error: { code: "ENCRYPTED", message: "Document is protected" }
  }) + "\n");
});
"#,
    );
    let client =
        ParserClient::with_program("node", [error_script.as_os_str()], Duration::from_secs(2));
    let error = client.parse("protected.hwp", 1024).unwrap_err();
    assert!(matches!(
        error,
        ParserError::Protocol {
            code: ParseErrorCode::Encrypted,
            ..
        }
    ));

    let slow_script = write_fake_sidecar(
        temp.path(),
        r#"
import readline from "node:readline";
readline.createInterface({ input: process.stdin }).on("line", () => {});
"#,
    );
    let slow_client =
        ParserClient::with_program("node", [slow_script.as_os_str()], Duration::from_millis(40));
    assert!(matches!(
        slow_client.parse("slow.pdf", 1024),
        Err(ParserError::Timeout)
    ));
}

#[test]
fn restarts_once_after_an_unexpected_exit() {
    let temp = TempDir::new().unwrap();
    let marker = temp.path().join("started");
    let script = write_fake_sidecar(
        temp.path(),
        r#"
import fs from "node:fs";
import readline from "node:readline";
const marker = process.argv[2];
readline.createInterface({ input: process.stdin }).on("line", line => {
  if (!fs.existsSync(marker)) {
    fs.writeFileSync(marker, "once");
    process.exit(23);
  }
  const request = JSON.parse(line);
  process.stdout.write(JSON.stringify({
    id: request.id,
    ok: true,
    document: {
      parserKind: "fake",
      title: null,
      markdown: "restarted",
      plainText: "restarted",
      blocks: [],
      metadata: {},
      warnings: []
    }
  }) + "\n");
});
"#,
    );
    let client = ParserClient::with_program(
        "node",
        [script.as_os_str(), marker.as_os_str()],
        Duration::from_secs(2),
    );

    assert_eq!(
        client.parse("restart.docx", 1024).unwrap().plain_text,
        "restarted"
    );
}

#[cfg(windows)]
#[test]
fn windows_children_use_no_console_creation_flags() {
    assert_eq!(
        everyfile_lib::parsing::windows_creation_flags(),
        0x0800_0000
    );
}

#[cfg(windows)]
#[test]
fn packaged_sidecar_is_gui_subsystem_and_parses_hwpx() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let binary = manifest
        .join("binaries")
        .join("everyfile-parser-x86_64-pc-windows-msvc.exe");
    let bytes = fs::read(&binary).expect("run node scripts/build-parser-sidecar.mjs first");
    let pe_offset = u32::from_le_bytes(bytes[0x3c..0x40].try_into().unwrap()) as usize;
    let subsystem = u16::from_le_bytes(bytes[pe_offset + 92..pe_offset + 94].try_into().unwrap());
    assert_eq!(subsystem, 2, "sidecar must use the Windows GUI subsystem");

    let fixture = manifest
        .parent()
        .unwrap()
        .join("vendor")
        .join("kordoc")
        .join("tests")
        .join("fixtures")
        .join("dummy.hwpx");
    let client = ParserClient::new(binary, Duration::from_secs(10));
    let document = client.parse(fixture, 20_000_000).unwrap();
    assert!(document.plain_text.contains("테스트"));
}
