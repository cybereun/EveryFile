use std::fs;
use std::path::Path;
#[cfg(windows)]
use std::process::{Command, Stdio};
#[cfg(windows)]
use std::thread;
use std::time::Duration;
#[cfg(windows)]
use std::time::Instant;

use everyfile_lib::parsing::{ParseErrorCode, ParserClient, ParserError};
use tempfile::TempDir;

fn write_fake_sidecar(directory: &Path, body: &str) -> std::path::PathBuf {
    let script = directory.join("fake-parser.mjs");
    fs::write(&script, body).unwrap();
    script
}

#[cfg(windows)]
fn process_is_running(pid: u32) -> bool {
    Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "if (Get-Process -Id {pid} -ErrorAction SilentlyContinue) {{ exit 0 }} else {{ exit 1 }}"
            ),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success()
}

#[cfg(windows)]
fn wait_for_process_exit(pid: u32) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while process_is_running(pid) {
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(20));
    }
    true
}

#[cfg(windows)]
fn force_kill_process(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
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
fn dropping_last_client_owner_terminates_the_sidecar_process() {
    let temp = TempDir::new().unwrap();
    let pid_file = temp.path().join("drop.pid");
    let script = write_fake_sidecar(
        temp.path(),
        r#"
import fs from "node:fs";
import readline from "node:readline";
fs.writeFileSync(process.argv[2], String(process.pid));
readline.createInterface({ input: process.stdin }).on("line", line => {
  const request = JSON.parse(line);
  process.stdout.write(JSON.stringify({
    id: request.id,
    ok: true,
    document: {
      parserKind: "fake",
      title: null,
      markdown: "drop",
      plainText: "drop",
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
        [script.as_os_str(), pid_file.as_os_str()],
        Duration::from_secs(2),
    );

    assert_eq!(client.parse("drop.hwpx", 1024).unwrap().plain_text, "drop");
    let pid = fs::read_to_string(&pid_file).unwrap().parse().unwrap();
    assert!(process_is_running(pid));

    drop(client);

    let exited = wait_for_process_exit(pid);
    if !exited {
        force_kill_process(pid);
    }
    assert!(exited, "sidecar process survived client shutdown");
}

#[cfg(windows)]
#[test]
fn stdout_read_failure_terminates_old_generation_before_retrying() {
    let temp = TempDir::new().unwrap();
    let generation_file = temp.path().join("generation");
    let pid_file = temp.path().join("generations.pid");
    let script = write_fake_sidecar(
        temp.path(),
        r#"
import fs from "node:fs";
import readline from "node:readline";
const generationFile = process.argv[2];
const pidFile = process.argv[3];
const generation = fs.existsSync(generationFile)
  ? Number(fs.readFileSync(generationFile, "utf8")) + 1
  : 1;
fs.writeFileSync(generationFile, String(generation));
fs.appendFileSync(pidFile, `${process.pid}\n`);
readline.createInterface({ input: process.stdin }).on("line", line => {
  if (generation === 1) {
    process.stdout.write(Buffer.from([0xff, 0x0a]), () => {
      process.stdout.destroy();
    });
    setInterval(() => {}, 1000);
    return;
  }
  const request = JSON.parse(line);
  process.stdout.write(JSON.stringify({
    id: request.id,
    ok: true,
    document: {
      parserKind: "fake",
      title: null,
      markdown: `generation-${generation}`,
      plainText: `generation-${generation}`,
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
        [
            script.as_os_str(),
            generation_file.as_os_str(),
            pid_file.as_os_str(),
        ],
        Duration::from_secs(2),
    );

    assert_eq!(
        client
            .parse("retry-after-eof.pdf", 1024)
            .unwrap()
            .plain_text,
        "generation-2"
    );
    let pids = fs::read_to_string(&pid_file)
        .unwrap()
        .lines()
        .map(|line| line.parse::<u32>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(pids.len(), 2);
    let old_exited = wait_for_process_exit(pids[0]);
    if !old_exited {
        force_kill_process(pids[0]);
    }
    assert!(process_is_running(pids[1]));

    drop(client);
    let new_exited = wait_for_process_exit(pids[1]);
    if !new_exited {
        force_kill_process(pids[1]);
    }
    assert!(
        old_exited,
        "stdout read failure left the old generation alive"
    );
    assert!(
        new_exited,
        "last client drop left the retry generation alive"
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
