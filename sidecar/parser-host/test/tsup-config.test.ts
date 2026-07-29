import { describe, expect, it } from "vitest";

import { rewritePinnedKordocCfb } from "../tsup.config.js";

const PINNED_SOURCE = [
  "var require2 = createRequire(import.meta.url);",
  'const cfb = require2("cfb");',
].join("\n");

describe("pinned Kordoc CFB bundle rewrite", () => {
  it("rewrites both expected compatibility patterns", () => {
    expect(rewritePinnedKordocCfb(PINNED_SOURCE)).toBe(
      ["var require2 = require;", 'const cfb = require("cfb");'].join("\n"),
    );
  });

  it("rejects a missing pinned-source pattern", () => {
    expect(() =>
      rewritePinnedKordocCfb('const cfb = require2("cfb");'),
    ).toThrow(/pinned Kordoc bundle drift/);
  });

  it("rejects a duplicated pinned-source pattern", () => {
    expect(() =>
      rewritePinnedKordocCfb(`${PINNED_SOURCE}\n${PINNED_SOURCE}`),
    ).toThrow(/pinned Kordoc bundle drift/);
  });
});
