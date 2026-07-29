import {
  parse,
  type ErrorCode as KordocErrorCode,
  type IRBlock,
  type ParseWarning,
} from "../../../vendor/kordoc/dist/index.js";

import type {
  AdapterResult,
  ParseErrorCode,
  ParsedDocument,
} from "./protocol.js";

type AdapterErrorCode = Exclude<
  ParseErrorCode,
  "INVALID_REQUEST" | "TIMEOUT"
>;

function mapKordocError(code: KordocErrorCode | undefined): AdapterErrorCode {
  switch (code) {
    case "UNSUPPORTED_FORMAT":
      return "UNSUPPORTED";
    case "ENCRYPTED":
    case "DRM_PROTECTED":
      return "ENCRYPTED";
    case "CORRUPTED":
    case "NO_SECTIONS":
      return "DAMAGED";
    case "DECOMPRESSION_BOMB":
    case "ZIP_BOMB":
      return "TOO_LARGE";
    case "IMAGE_BASED_PDF":
      return "IMAGE_BASED_PDF";
    case "EMPTY_INPUT":
    case "PARSE_ERROR":
    case "MISSING_DEPENDENCY":
    default:
      return "INTERNAL";
  }
}

function blockToText(block: IRBlock): string {
  const ownText = block.text?.trim() ?? "";
  const childText =
    block.children?.map(blockToText).filter(Boolean).join("\n") ?? "";
  const tableText =
    block.table?.cells
      .map((row) => row.map((cell) => cell.text.trim()).filter(Boolean).join("\t"))
      .filter(Boolean)
      .join("\n") ?? "";
  return [ownText, tableText, childText].filter(Boolean).join("\n");
}

function normalizeBlock(block: IRBlock): Record<string, unknown> {
  const normalized: Record<string, unknown> = { type: block.type };
  if (block.text !== undefined) normalized.text = block.text;
  if (block.table !== undefined) normalized.table = block.table;
  if (block.level !== undefined) normalized.level = block.level;
  if (block.pageNumber !== undefined) normalized.pageNumber = block.pageNumber;
  if (block.bbox !== undefined) normalized.bbox = block.bbox;
  if (block.style !== undefined) normalized.style = block.style;
  if (block.listType !== undefined) normalized.listType = block.listType;
  if (block.children !== undefined) {
    normalized.children = block.children.map(normalizeBlock);
  }
  if (block.href !== undefined) normalized.href = block.href;
  if (block.footnoteText !== undefined) {
    normalized.footnoteText = block.footnoteText;
  }
  if (block.imageData !== undefined) {
    normalized.image = {
      mimeType: block.imageData.mimeType,
      filename: block.imageData.filename ?? null,
    };
  }
  return normalized;
}

function normalizeWarning(warning: ParseWarning): Record<string, unknown> {
  const normalized: Record<string, unknown> = {
    code: warning.code,
    message: warning.message,
  };
  if (warning.page !== undefined) normalized.page = warning.page;
  return normalized;
}

export async function parseWithKordoc(path: string): Promise<AdapterResult> {
  const result = await parse(path, {
    removeHeaderFooter: true,
    formulaOcr: false,
  });
  if (!result.success) {
    return {
      ok: false,
      error: { code: mapKordocError(result.code) },
    };
  }

  const document: ParsedDocument = {
    parserKind: "kordoc",
    title: result.metadata?.title ?? null,
    markdown: result.markdown,
    plainText: result.blocks.map(blockToText).filter(Boolean).join("\n\n"),
    blocks: result.blocks.map(normalizeBlock),
    metadata: { ...(result.metadata ?? {}) },
    warnings: (result.warnings ?? []).map(normalizeWarning),
  };
  return { ok: true, document };
}
