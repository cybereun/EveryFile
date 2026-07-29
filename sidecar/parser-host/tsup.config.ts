import { readFile } from "node:fs/promises";
import type { Plugin } from "esbuild";
import { defineConfig } from "tsup";

const REQUIRE_FACTORY = "var require2 = createRequire(import.meta.url);";
const CFB_IMPORT = 'require2("cfb")';

function occurrenceCount(source: string, pattern: string): number {
  return source.split(pattern).length - 1;
}

export function rewritePinnedKordocCfb(source: string): string {
  if (
    occurrenceCount(source, REQUIRE_FACTORY) !== 1 ||
    occurrenceCount(source, CFB_IMPORT) !== 1
  ) {
    throw new Error(
      "pinned Kordoc bundle drift: expected each CFB compatibility pattern exactly once",
    );
  }
  return source
    .replace(REQUIRE_FACTORY, "var require2 = require;")
    .replace(CFB_IMPORT, 'require("cfb")');
}

const bundleKordocCfb: Plugin = {
  name: "bundle-kordoc-cfb",
  setup(build) {
    build.onLoad(
      { filter: /vendor[\\/]kordoc[\\/]dist[\\/]index\.js$/ },
      async (arguments_) => {
        const source = await readFile(arguments_.path, "utf8");
        return {
          contents: rewritePinnedKordocCfb(source),
          loader: "js",
        };
      },
    );
  },
};

export default defineConfig({
  entry: ["src/main.ts"],
  format: ["cjs"],
  platform: "node",
  target: "node22",
  splitting: false,
  outDir: "dist",
  clean: true,
  esbuildPlugins: [bundleKordocCfb],
  noExternal: ["cfb"],
  external: [
    "onnxruntime-node",
    "@huggingface/transformers",
    "@hyzyla/pdfium",
    "sharp",
    "puppeteer-core",
    "canvas",
  ],
});
