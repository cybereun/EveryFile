import { stat } from "node:fs/promises";

import { z } from "zod";

export const ParseRequestSchema = z
  .object({
    id: z.string().min(1),
    operation: z.literal("parse"),
    path: z.string().min(1),
    options: z
      .object({
        maxBytes: z.number().int().positive().max(500_000_000),
      })
      .strict(),
  })
  .strict();

export type ParseRequest = z.infer<typeof ParseRequestSchema>;

export const ParseErrorCodeSchema = z.enum([
  "INVALID_REQUEST",
  "UNSUPPORTED",
  "ENCRYPTED",
  "DAMAGED",
  "TIMEOUT",
  "TOO_LARGE",
  "IMAGE_BASED_PDF",
  "INTERNAL",
]);

export type ParseErrorCode = z.infer<typeof ParseErrorCodeSchema>;

const JsonObjectSchema = z.record(z.string(), z.unknown());

export const ParsedDocumentSchema = z
  .object({
    parserKind: z.literal("kordoc"),
    title: z.string().nullable(),
    markdown: z.string(),
    plainText: z.string(),
    blocks: z.array(JsonObjectSchema),
    metadata: JsonObjectSchema,
    warnings: z.array(JsonObjectSchema),
  })
  .strict();

export type ParsedDocument = z.infer<typeof ParsedDocumentSchema>;

const ParseSuccessSchema = z
  .object({
    id: z.string().nullable(),
    ok: z.literal(true),
    document: ParsedDocumentSchema,
  })
  .strict();

const ParseFailureSchema = z
  .object({
    id: z.string().nullable(),
    ok: z.literal(false),
    error: z
      .object({
        code: ParseErrorCodeSchema,
        message: z.string(),
      })
      .strict(),
  })
  .strict();

export const ParseResponseSchema = z.discriminatedUnion("ok", [
  ParseSuccessSchema,
  ParseFailureSchema,
]);

export type ParseSuccess = z.infer<typeof ParseSuccessSchema> & {
  error?: never;
};
export type ParseFailure = z.infer<typeof ParseFailureSchema> & {
  document?: never;
};
export type ParseResponse = ParseSuccess | ParseFailure;

export type AdapterResult =
  | { ok: true; document: ParsedDocument }
  | {
      ok: false;
      error: { code: Exclude<ParseErrorCode, "INVALID_REQUEST" | "TIMEOUT"> };
    };

export interface ParseDependencies {
  parseDocument: (path: string) => Promise<AdapterResult>;
  statFile: typeof stat;
  timeoutMs: number;
}

const DEFAULT_TIMEOUT_MS = 120_000;

async function defaultParseDocument(path: string): Promise<AdapterResult> {
  const { parseWithKordoc } = await import("./kordoc-adapter.js");
  return parseWithKordoc(path);
}

function failure(
  id: string | null,
  code: ParseErrorCode,
): ParseFailure {
  const messages: Record<ParseErrorCode, string> = {
    INVALID_REQUEST: "The parser request is invalid.",
    UNSUPPORTED: "The document format is not supported.",
    ENCRYPTED: "The document is encrypted or protected.",
    DAMAGED: "The document is damaged.",
    TIMEOUT: "Document parsing timed out.",
    TOO_LARGE: "The document exceeds the configured size limit.",
    IMAGE_BASED_PDF: "The PDF requires OCR.",
    INTERNAL: "Document parsing failed.",
  };
  return {
    id,
    ok: false,
    error: { code, message: messages[code] },
  };
}

function requestId(value: unknown): string | null {
  if (
    typeof value === "object" &&
    value !== null &&
    "id" in value &&
    typeof value.id === "string"
  ) {
    return value.id;
  }
  return null;
}

async function withTimeout<T>(
  operation: Promise<T>,
  timeoutMs: number,
): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_resolve, reject) => {
    timer = setTimeout(() => reject(new Error("PARSER_TIMEOUT")), timeoutMs);
  });
  try {
    return await Promise.race([operation, timeout]);
  } finally {
    if (timer !== undefined) {
      clearTimeout(timer);
    }
  }
}

export async function handleRequest(
  input: unknown,
  overrides: Partial<ParseDependencies> = {},
): Promise<ParseResponse> {
  const parsed = ParseRequestSchema.safeParse(input);
  if (!parsed.success) {
    return failure(requestId(input), "INVALID_REQUEST");
  }
  const request = parsed.data;
  const dependencies: ParseDependencies = {
    parseDocument: overrides.parseDocument ?? defaultParseDocument,
    statFile: overrides.statFile ?? stat,
    timeoutMs: overrides.timeoutMs ?? DEFAULT_TIMEOUT_MS,
  };

  try {
    const file = await dependencies.statFile(request.path);
    if (!file.isFile()) {
      return failure(request.id, "INTERNAL");
    }
    if (file.size > request.options.maxBytes) {
      return failure(request.id, "TOO_LARGE");
    }

    const result = await withTimeout(
      dependencies.parseDocument(request.path),
      dependencies.timeoutMs,
    );
    if (!result.ok) {
      return failure(request.id, result.error.code);
    }
    return ParseResponseSchema.parse({
      id: request.id,
      ok: true,
      document: result.document,
    }) as ParseSuccess;
  } catch (error) {
    if (error instanceof Error && error.message === "PARSER_TIMEOUT") {
      return failure(request.id, "TIMEOUT");
    }
    process.stderr.write("[everyfile-parser] adapter failure\n");
    return failure(request.id, "INTERNAL");
  }
}

export async function handleLine(
  line: string,
  overrides: Partial<ParseDependencies> = {},
): Promise<ParseResponse> {
  let value: unknown;
  try {
    value = JSON.parse(line);
  } catch {
    return failure(null, "INVALID_REQUEST");
  }
  return handleRequest(value, overrides);
}

class RequestLimiter {
  private active = 0;
  private readonly waiting: Array<() => void> = [];

  constructor(private readonly maximum: number) {}

  async run<T>(operation: () => Promise<T>): Promise<T> {
    if (this.active >= this.maximum) {
      await new Promise<void>((resolvePromise) => {
        this.waiting.push(resolvePromise);
      });
    }
    this.active += 1;
    try {
      return await operation();
    } finally {
      this.active -= 1;
      this.waiting.shift()?.();
    }
  }
}

export function createRequestHandler(
  overrides: Partial<ParseDependencies> = {},
): (line: string) => Promise<ParseResponse> {
  const limiter = new RequestLimiter(3);
  return (line) => limiter.run(() => handleLine(line, overrides));
}
