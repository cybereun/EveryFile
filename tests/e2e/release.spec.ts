import { execFileSync, spawn, spawnSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterAll, describe, expect, it } from "vitest";

const enabled =
  process.platform === "win32" &&
  process.env.EVERYFILE_RELEASE_ACCEPTANCE === "1";
const root = path.resolve(import.meta.dirname, "..", "..");
const release = path.join(root, "artifacts", "release");
const version = "1.0.2";
const installer = path.join(release, `EveryFile-Setup-v${version}.exe`);
const portableZip = path.join(release, `EveryFile-Portable-v${version}.zip`);
const temporary = enabled
  ? mkdtempSync(path.join(tmpdir(), "everyfile-release-"))
  : "";

afterAll(() => {
  if (
    temporary &&
    path.resolve(temporary).startsWith(path.resolve(tmpdir()) + path.sep)
  ) {
    try {
      rmSync(temporary, { recursive: true, force: true, maxRetries: 5, retryDelay: 500 });
    } catch {
      // Windows may briefly retain WebView2 files after the smoke process exits.
    }
  }
});

function findFile(directory: string, name: string): string | undefined {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const candidate = path.join(directory, entry.name);
    if (entry.isFile() && entry.name === name) return candidate;
    if (entry.isDirectory()) {
      const nested = findFile(candidate, name);
      if (nested) return nested;
    }
  }
  return undefined;
}

async function smoke(executable: string, profile: string) {
  const child = spawn(executable, [], {
    cwd: path.dirname(executable),
    env: { ...process.env, LOCALAPPDATA: profile },
    stdio: "ignore",
    windowsHide: true,
  });
  await new Promise((resolve) => setTimeout(resolve, 8_000));
  expect(child.exitCode).toBeNull();
  child.kill();
  await new Promise<void>((resolve) => {
    if (child.exitCode !== null) {
      resolve();
      return;
    }
    child.once("exit", () => resolve());
    setTimeout(resolve, 10_000);
  });
}

describe.skipIf(!enabled)("EveryFile clean release distributions", () => {
  it("runs the portable distribution with both local sidecars", async () => {
    expect(existsSync(portableZip)).toBe(true);
    const destination = path.join(temporary, "portable");
    mkdirSync(destination, { recursive: true });
    execFileSync(
      "powershell",
      [
        "-NoProfile",
        "-Command",
        "& { param($archive, $destination) Expand-Archive -LiteralPath $archive -DestinationPath $destination -Force }",
        portableZip,
        destination,
      ],
      { windowsHide: true },
    );
    const executable = findFile(destination, "EveryFile.exe");
    expect(executable).toBeTruthy();
    expect(findFile(destination, "everyfile-parser.exe")).toBeTruthy();
    expect(findFile(destination, "everyfile-ocr.exe")).toBeTruthy();
    await smoke(executable!, path.join(temporary, "portable-profile"));
  }, 300_000);

  it("silently installs to a clean location and launches without a console", async () => {
    expect(existsSync(installer)).toBe(true);
    const destination = path.join(temporary, "installed");
    const outcome = spawnSync(
      installer,
      ["/S", `/D=${destination}`],
      { timeout: 300_000, windowsHide: true, stdio: "ignore" },
    );
    expect(outcome.status).toBe(0);
    const executable = findFile(destination, "EveryFile.exe");
    expect(executable).toBeTruthy();
    await smoke(executable!, path.join(temporary, "installed-profile"));
  }, 300_000);
});
