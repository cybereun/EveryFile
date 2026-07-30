import type { SearchMode, SearchTermMode } from "../../lib/types";

export type SearchOption = SearchTermMode;
export type SearchSort =
  | "relevance"
  | "confidence"
  | "newest"
  | "oldest"
  | "name"
  | "size";

export interface SearchFilters {
  mode: SearchMode;
  option: SearchOption;
  sort: SearchSort;
  extensions: string[];
  extensionless: boolean;
  modifiedAfter: string | null;
  modifiedBefore: string | null;
  folderIds: string[];
  includeFilename: boolean;
}

export const DEFAULT_SEARCH_FILTERS: SearchFilters = {
  mode: "keyword",
  option: "all",
  sort: "relevance",
  extensions: [],
  extensionless: false,
  modifiedAfter: null,
  modifiedBefore: null,
  folderIds: [],
  includeFilename: true,
};

export type QueryClauseKind =
  | "term"
  | "phrase"
  | "exclude"
  | "extension"
  | "path"
  | "after"
  | "before"
  | "near"
  | "or";

export interface QueryClause {
  kind: QueryClauseKind;
  raw: string;
  value: string;
  values?: string[];
}

export interface ParsedSearchQuery {
  clauses: QueryClause[];
  positiveGroups: string[][];
  errors: string[];
}

interface RawToken {
  raw: string;
  closed: boolean;
}

function scanTokens(input: string): RawToken[] {
  const tokens: RawToken[] = [];
  let start = -1;
  let quote = false;
  let escaped = false;

  const push = (end: number) => {
    if (start >= 0) {
      tokens.push({ raw: input.slice(start, end), closed: !quote });
      start = -1;
    }
  };

  for (let index = 0; index < input.length; index += 1) {
    const character = input[index];
    if (start < 0) {
      if (/\s/.test(character)) continue;
      start = index;
    }
    if (escaped) {
      escaped = false;
      continue;
    }
    if (quote && character === "\\") {
      escaped = true;
      continue;
    }
    if (character === '"') {
      quote = !quote;
      continue;
    }
    if (!quote && /\s/.test(character)) push(index);
  }
  push(input.length);
  return tokens;
}

function decodeQuoted(value: string) {
  if (!(value.startsWith('"') && value.endsWith('"'))) return value;
  return value
    .slice(1, -1)
    .replace(/\\"/g, '"')
    .replace(/\\\\/g, "\\");
}

function clauseFromToken(token: RawToken): QueryClause {
  const { raw } = token;
  if (raw === "OR") return { kind: "or", raw, value: "OR" };
  if (raw.startsWith('"') && raw.endsWith('"')) {
    return { kind: "phrase", raw, value: decodeQuoted(raw) };
  }

  const operator = /^([a-z]+):(.*)$/i.exec(raw);
  if (operator) {
    const name = operator[1].toLowerCase();
    const encoded = operator[2];
    if (name === "ext") {
      const values = decodeQuoted(encoded)
        .split(",")
        .map((value) => value.trim().replace(/^\./, "").toLowerCase())
        .filter(Boolean);
      return {
        kind: "extension",
        raw,
        value: values.join(","),
        values,
      };
    }
    if (["path", "after", "before"].includes(name)) {
      return {
        kind: name as "path" | "after" | "before",
        raw,
        value: decodeQuoted(encoded),
      };
    }
  }
  if (/^~\d+$/.test(raw)) {
    return { kind: "near", raw, value: raw.slice(1) };
  }
  if (raw.startsWith("-") && raw.length > 1) {
    return {
      kind: "exclude",
      raw,
      value: decodeQuoted(raw.slice(1)),
    };
  }
  return { kind: "term", raw, value: raw };
}

export function parseSearchQuery(input: string): ParsedSearchQuery {
  const scanned = scanTokens(input);
  const clauses = scanned.map(clauseFromToken);
  const errors: string[] = [];
  if (scanned.some((token) => !token.closed)) errors.push("따옴표가 닫히지 않았습니다.");

  const positiveGroups: string[][] = [[]];
  let groupIndex = 0;
  let previousWasOr = false;
  for (const clause of clauses) {
    if (clause.kind === "or") {
      if (
        previousWasOr ||
        positiveGroups[groupIndex].length === 0
      ) {
        errors.push("OR는 검색어 사이에 입력해야 합니다.");
      }
      previousWasOr = true;
      groupIndex += 1;
      positiveGroups[groupIndex] = [];
      continue;
    }
    if (clause.kind === "term" || clause.kind === "phrase") {
      positiveGroups[groupIndex].push(clause.value);
      previousWasOr = false;
    }
  }
  if (previousWasOr) errors.push("OR 뒤에 검색어가 필요합니다.");
  return {
    clauses,
    positiveGroups: positiveGroups.filter((group) => group.length > 0),
    errors: [...new Set(errors)],
  };
}

export function serializeSearchQuery(parsed: ParsedSearchQuery) {
  return parsed.clauses.map((clause) => clause.raw).join(" ");
}

export function queryForTermMode(query: string, mode: SearchTermMode) {
  const parsed = parseSearchQuery(query);
  const clauses = parsed.clauses
    .filter((clause) => clause.kind !== "or" && clause.kind !== "near")
    .map((clause) => {
      if (clause.kind !== "exclude" || mode === "exclude") return clause;
      return clauseFromToken({
        raw: clause.raw.slice(1),
        closed: true,
      });
    });
  return serializeSearchQuery({ ...parsed, clauses });
}

export function removeQueryClause(
  query: string,
  predicate: (clause: QueryClause) => boolean,
) {
  const parsed = parseSearchQuery(query);
  return serializeSearchQuery({
    ...parsed,
    clauses: parsed.clauses.filter((clause) => !predicate(clause)),
  });
}

export function extensionsFromQuery(query: string) {
  const extensions: string[] = [];
  for (const clause of parseSearchQuery(query).clauses) {
    if (clause.kind !== "extension") continue;
    for (const extension of clause.values ?? []) {
      if (!extensions.includes(extension)) extensions.push(extension);
    }
  }
  return extensions;
}

export function withExtensionQuery(query: string, extensions: string[]) {
  const parsed = parseSearchQuery(query);
  const normalized = [...new Set(extensions.map((value) => value.toLowerCase()))];
  const clauses = parsed.clauses.filter((clause) => clause.kind !== "extension");
  if (normalized.length > 0) {
    clauses.push({
      kind: "extension",
      raw: `ext:${normalized.join(",")}`,
      value: normalized.join(","),
      values: normalized,
    });
  }
  return serializeSearchQuery({ ...parsed, clauses });
}

export function explicitOptionFromQuery(query: string): SearchOption | null {
  const clauses = parseSearchQuery(query).clauses;
  if (clauses.some((clause) => clause.kind === "or")) return "any";
  if (clauses.some((clause) => clause.kind === "near")) return "near";
  const positive = clauses.filter(
    (clause) => clause.kind === "term" || clause.kind === "phrase",
  );
  if (
    positive.length === 0 &&
    clauses.some((clause) => clause.kind === "exclude")
  ) {
    return "exclude";
  }
  if (positive.length === 1 && positive[0].kind === "phrase") return "exact";
  return null;
}

export function structuredValuesFromQuery(query: string) {
  const parsed = parseSearchQuery(query);
  const value = (kind: QueryClauseKind) =>
    parsed.clauses.find((clause) => clause.kind === kind)?.value ?? null;
  return {
    extensions: extensionsFromQuery(query),
    modifiedAfter: value("after"),
    modifiedBefore: value("before"),
  };
}

export function buildBackendQuery(query: string) {
  return serializeSearchQuery(parseSearchQuery(query));
}

export function hasSearchCriteria(query: string, filters: SearchFilters) {
  return Boolean(
    query.trim() ||
      filters.extensions.length ||
      filters.extensionless ||
      filters.modifiedAfter ||
      filters.modifiedBefore ||
      filters.folderIds.length,
  );
}
