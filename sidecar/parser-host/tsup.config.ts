import { readFile } from "node:fs/promises";
import type { Plugin } from "esbuild";
import { defineConfig } from "tsup";

const bundleKordocCfb: Plugin = {
  name: "bundle-kordoc-cfb",
  setup(build) {
    build.onLoad(
      { filter: /vendor[\\/]kordoc[\\/]dist[\\/]index\.js$/ },
      async (arguments_) => {
        const source = await readFile(arguments_.path, "utf8");
        return {
          contents: source
            .replace(
              "var require2 = createRequire(import.meta.url);",
              "var require2 = require;",
            )
            .replace('require2("cfb")', 'require("cfb")'),
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
