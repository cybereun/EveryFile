import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

const root = path.resolve(import.meta.dirname, "..", "..");
const productionConfig = JSON.parse(
  readFileSync(path.join(root, "src-tauri", "tauri.conf.json"), "utf8"),
) as { identifier: string };
const e2eConfig = JSON.parse(
  readFileSync(path.join(root, "src-tauri", "tauri.e2e.conf.json"), "utf8"),
) as { identifier: string };
const wdioConfig = readFileSync(path.join(root, "wdio.conf.ts"), "utf8");

describe("E2E application-data isolation", () => {
  it("uses a different Tauri identity from the installed application", () => {
    expect(e2eConfig.identifier).toBe("com.cybereun.everyfile.e2e");
    expect(e2eConfig.identifier).not.toBe(productionConfig.identifier);
  });

  it("requires a dedicated temporary data directory for each Webdriver run", () => {
    expect(wdioConfig).toContain("mkdtempSync");
    expect(wdioConfig).toContain('"--e2e-data-dir"');
    expect(wdioConfig).toContain("rmSync(e2eDataDir");
  });
});
