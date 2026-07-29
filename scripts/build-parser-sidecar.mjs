import {
  copyFileSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const kordocDirectory = join(repositoryRoot, "vendor", "kordoc");
const sidecarDirectory = join(repositoryRoot, "sidecar", "parser-host");
const sourceExecutable = join(sidecarDirectory, "everyfile-parser.exe");
const npmCli = join(
  dirname(process.execPath),
  "node_modules",
  "npm",
  "bin",
  "npm-cli.js",
);
const pkgCli = join(
  sidecarDirectory,
  "node_modules",
  "@yao-pkg",
  "pkg",
  "lib-es5",
  "bin.js",
);

function run(command, arguments_, options = {}) {
  const result = spawnSync(command, arguments_, {
    cwd: options.cwd ?? repositoryRoot,
    encoding: options.capture ? "utf8" : undefined,
    stdio: options.capture ? ["ignore", "pipe", "inherit"] : "inherit",
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${command} failed with exit code ${result.status}`);
  }
  return options.capture ? result.stdout.trim() : "";
}

function markAsWindowsGuiExecutable(path) {
  const executable = readFileSync(path);
  const peOffset = executable.readUInt32LE(0x3c);
  if (executable.toString("ascii", peOffset, peOffset + 4) !== "PE\0\0") {
    throw new Error("pkg output is not a Windows PE executable");
  }
  const optionalHeader = peOffset + 24;
  const magic = executable.readUInt16LE(optionalHeader);
  if (magic !== 0x10b && magic !== 0x20b) {
    throw new Error("pkg output has an unsupported PE optional header");
  }
  executable.writeUInt16LE(2, optionalHeader + 68);
  writeFileSync(path, executable);
  const verified = readFileSync(path);
  if (verified.readUInt16LE(optionalHeader + 68) !== 2) {
    throw new Error("failed to mark parser sidecar as a Windows GUI executable");
  }
}

run(process.execPath, [npmCli, "ci"], { cwd: kordocDirectory });
run(process.execPath, [npmCli, "run", "build"], { cwd: kordocDirectory });
run(process.execPath, [npmCli, "ci"], { cwd: sidecarDirectory });
run(process.execPath, [npmCli, "run", "bundle"], { cwd: sidecarDirectory });
run(
  process.execPath,
  [
    pkgCli,
    "dist/main.cjs",
    "--target",
    "node22-win-x64",
    "--fallback-to-source",
    "--output",
    sourceExecutable,
  ],
  { cwd: sidecarDirectory },
);
markAsWindowsGuiExecutable(sourceExecutable);

const target = run("rustc", ["--print", "host-tuple"], { capture: true });
if (!/^[A-Za-z0-9_.-]+$/.test(target)) {
  throw new Error("rustc returned an invalid host tuple");
}
const binaryDirectory = join(repositoryRoot, "src-tauri", "binaries");
const targetExecutable = join(
  binaryDirectory,
  `everyfile-parser-${target}.exe`,
);
mkdirSync(binaryDirectory, { recursive: true });
copyFileSync(sourceExecutable, targetExecutable);

process.stdout.write(`parser sidecar: ${targetExecutable}\n`);
