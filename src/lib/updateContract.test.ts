// @vitest-environment node
// @ts-expect-error Node built-ins are supplied by Vitest, not the browser app.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const read = (path: string): string => readFileSync(path, "utf8");
const config = JSON.parse(read("src-tauri/tauri.conf.json"));
const releaseWorkflow = read(".github/workflows/release.yml");
const updaterPublicKey =
  "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDk2OTQ2OUU1REI0QjY4MEEKUldRS2FFdmI1V21VbG9STk54aDBscVVneWE1M1ZjTFFwY0hEZWZIS1FjcDE2QkZPYjlpaU42ZmoK";

describe("installed-client update compatibility", () => {
  it("keeps the app identity, trusted signing key and update endpoint", () => {
    expect(config.identifier).toBe("com.cybereun.everyfile");
    expect(config.plugins.updater.pubkey).toBe(updaterPublicKey);
    expect(config.plugins.updater.endpoints).toEqual([
      "https://github.com/cybereun/EveryFile/releases/latest/download/latest.json",
    ]);
  });

  it("keeps package, native binary and installer versions in sync", () => {
    const pkg = JSON.parse(read("package.json"));
    const lock = JSON.parse(read("package-lock.json"));
    const cargoVersion = read("src-tauri/Cargo.toml").match(
      /^version = "([^"]+)"/m,
    )?.[1];
    expect(config.version).toBe(pkg.version);
    expect(cargoVersion).toBe(pkg.version);
    expect(lock.version).toBe(pkg.version);
    expect(lock.packages[""].version).toBe(pkg.version);
  });

  it("retains signed NSIS artifacts and the existing Windows restart hooks", () => {
    expect(config.bundle.createUpdaterArtifacts).toBe(true);
    expect(config.bundle.targets).toContain("nsis");
    expect(config.bundle.windows.nsis.installMode).toBe("currentUser");
    expect(config.plugins.updater.windows.installMode).toBe("passive");
    expect(config.bundle.windows.nsis.installerHooks).toBe(
      "installer/update-hooks.nsh",
    );
    expect(
      read("src-tauri/installer/update-hooks.nsh").trim().length,
    ).toBeGreaterThan(0);
  });

  it("publishes every artifact required by installed clients and rejects mismatched tags", () => {
    for (const asset of [
      "latest.json",
      "EveryFile_${version}_x64-setup.exe",
      "EveryFile_${version}_x64-setup.exe.sig",
    ]) {
      expect(releaseWorkflow).toContain(asset);
    }
    expect(releaseWorkflow).toContain("Verify release tag matches application version");
    expect(releaseWorkflow).toContain("$env:GITHUB_REF_NAME -ne $expectedTag");
  });
});
