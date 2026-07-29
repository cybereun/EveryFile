import { createInterface } from "node:readline";

import { createRequestHandler } from "./protocol.js";

export async function runParserHost(): Promise<void> {
  const handle = createRequestHandler();
  const lines = createInterface({
    input: process.stdin,
    crlfDelay: Number.POSITIVE_INFINITY,
    terminal: false,
  });

  lines.on("line", (line) => {
    void handle(line)
      .then((response) => {
        process.stdout.write(`${JSON.stringify(response)}\n`);
      })
      .catch(() => {
        process.stderr.write("[everyfile-parser] response dispatch failed\n");
      });
  });
}

if (process.env.VITEST === undefined) {
  void runParserHost().catch(() => {
    process.stderr.write("[everyfile-parser] parser host terminated\n");
    process.exitCode = 1;
  });
}
