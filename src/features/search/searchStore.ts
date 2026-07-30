import type { SearchMode } from "../../lib/types";

export type SearchOption = "all" | "any" | "exact" | "exclude" | "near";
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
  modifiedAfter: string | null;
  modifiedBefore: string | null;
  folderIds: string[];
  includeFilename: boolean;
  withinResults: string;
}

export const DEFAULT_SEARCH_FILTERS: SearchFilters = {
  mode: "keyword",
  option: "all",
  sort: "relevance",
  extensions: [],
  modifiedAfter: null,
  modifiedBefore: null,
  folderIds: [],
  includeFilename: true,
  withinResults: "",
};

const EXTENSION_OPERATOR = /(?:^|\s)ext:([a-z0-9]+(?:,[a-z0-9]+)*)/gi;

export function extensionsFromQuery(query: string) {
  const extensions: string[] = [];
  for (const match of query.matchAll(EXTENSION_OPERATOR)) {
    for (const extension of match[1].toLowerCase().split(",")) {
      if (!extensions.includes(extension)) extensions.push(extension);
    }
  }
  return extensions;
}

export function withExtensionQuery(query: string, extensions: string[]) {
  const withoutExtensions = query
    .replace(EXTENSION_OPERATOR, " ")
    .replace(/\s+/g, " ")
    .trim();
  const normalized = [...new Set(extensions.map((value) => value.toLowerCase()))];
  const operator = normalized.length > 0 ? `ext:${normalized.join(",")}` : "";
  return [withoutExtensions, operator].filter(Boolean).join(" ");
}

function splitVisibleQuery(query: string) {
  const operators: string[] = [];
  const plain = query
    .replace(EXTENSION_OPERATOR, (match) => {
      operators.push(match.trim());
      return " ";
    })
    .replace(/\s+/g, " ")
    .trim();
  return { plain, operators };
}

function quote(value: string) {
  return `"${value.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
}

export function buildBackendQuery(query: string, option: SearchOption) {
  const { plain, operators } = splitVisibleQuery(query);
  const words = plain.split(/\s+/).filter(Boolean);
  let expression = plain;

  if (plain) {
    switch (option) {
      case "any":
        expression = words.join(" OR ");
        break;
      case "exact":
        expression = quote(plain);
        break;
      case "exclude":
        expression = words.map((word) => `-${word}`).join(" ");
        break;
      case "near":
        expression = words.length > 1 ? `${words.join(" ")} ~5` : plain;
        break;
      case "all":
        break;
    }
  }

  return [expression, ...operators].filter(Boolean).join(" ");
}

export function hasSearchCriteria(query: string, filters: SearchFilters) {
  return Boolean(
    query.trim() ||
      filters.extensions.length ||
      filters.modifiedAfter ||
      filters.modifiedBefore ||
      filters.folderIds.length,
  );
}
