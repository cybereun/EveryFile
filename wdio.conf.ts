import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(fileURLToPath(import.meta.url));
const binary = path.join(root, "src-tauri", "target", "debug", "EveryFile.exe");
const fixture = path.join(root, "tests", "fixtures", "folder-search");
const appArgs = [
  "--e2e-reset-state",
  "--e2e-register-fixture-folder",
  fixture,
  "--e2e-enable-ocr",
];

export const config = {
  runner: "local",
  specs: ["./tests/e2e/**/*.e2e.ts"],
  maxInstances: 1,
  services: [
    [
      "@wdio/tauri-service",
      {
        appBinaryPath: binary,
        appArgs,
        driverProvider: "embedded",
      },
    ],
  ],
  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": { application: binary, args: appArgs },
    },
  ],
  logLevel: "warn",
  waitforTimeout: 15_000,
  connectionRetryTimeout: 90_000,
  connectionRetryCount: 1,
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: { ui: "bdd", timeout: 600_000 },
};
