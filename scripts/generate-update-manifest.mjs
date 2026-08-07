import { copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const config = JSON.parse(
  await readFile(path.join(repoRoot, "src-tauri", "tauri.conf.json"), "utf8"),
);
const version = String(config.version);
const installerName = `EveryFile_${version}_x64-setup.exe`;
const installerPath = path.join(
  repoRoot,
  "src-tauri",
  "target",
  "release",
  "bundle",
  "nsis",
  installerName,
);
const signaturePath = `${installerPath}.sig`;
const releaseRoot = path.join(repoRoot, "artifacts", "release");
await mkdir(releaseRoot, { recursive: true });

const signature = (await readFile(signaturePath, "utf8")).trim();
if (!signature) throw new Error(`Updater signature is empty: ${signaturePath}`);

// Keep the exact Tauri-generated installer name for the updater asset. The
// friendly installer name remains available alongside it for direct downloads.
await copyFile(installerPath, path.join(releaseRoot, installerName));
await copyFile(signaturePath, path.join(releaseRoot, `${installerName}.sig`));

const releaseNotes = await readFile(path.join(repoRoot, "RELEASE_NOTES.md"), "utf8");
const manifest = {
  version,
  notes: releaseNotes.trim(),
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature,
      url: `https://github.com/cybereun/EveryFile/releases/download/v${version}/${installerName}`,
    },
  },
};
await writeFile(
  path.join(releaseRoot, "latest.json"),
  `${JSON.stringify(manifest, null, 2)}\n`,
  "utf8",
);
console.log(JSON.stringify({ version, manifest: path.join(releaseRoot, "latest.json"), installer: installerName }, null, 2));
