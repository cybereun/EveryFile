export type SearchMode = "keyword" | "filename";
export type SearchTermMode = "all" | "any" | "exact" | "exclude" | "near";
export type SearchMatchKind = "filename" | "content" | "both" | "metadata";

export interface SearchRequest {
  requestId: string;
  query: string;
  mode: SearchMode;
  folderIds: string[];
  extensions: string[];
  extensionless?: boolean;
  modifiedAfter: string | null;
  modifiedBefore: string | null;
  includeFilename: boolean;
  termMode: SearchTermMode;
  privateSearch: boolean;
  sort: string;
  limit: number;
  offset: number;
}

export interface SearchHit {
  documentId: string;
  fileName: string;
  path: string;
  extension: string;
  sizeBytes: number;
  modifiedAt: string;
  snippet: string | null;
  score: number;
  matchKind: SearchMatchKind;
}

export interface FolderRecord {
  id: string;
  canonicalPath: string;
  displayName: string;
  documentCount: number;
  indexState: string;
}

export interface DocumentRecord {
  id: string;
  folderId: string;
  canonicalPath: string;
  fileName: string;
  extension: string;
  sizeBytes: number;
  modifiedAt: string;
  contentHash: string | null;
  parserKind: string | null;
  parseState: string;
  parseErrorCode: string | null;
  indexedAt: string | null;
}

export interface SearchResponse {
  requestId: string;
  hits: SearchHit[];
  total: number;
  elapsedMs: number;
  appliedFilters: string[];
  hasMore: boolean;
}

export interface PreviewBlock {
  type: "paragraph" | "table" | "heading" | "list" | "image" | "separator";
  text: string;
  level: number | null;
  pageNumber: number | null;
  href?: string | null;
  listType?: "ordered" | "unordered" | null;
  children?: PreviewBlock[];
  table?: PreviewTable | null;
}

export interface PreviewTable {
  rows: number;
  cols: number;
  hasHeader: boolean;
  cells: PreviewCell[][];
}

export interface PreviewCell {
  text: string;
  colSpan: number;
  rowSpan: number;
}

export interface PreviewWarning {
  code: string;
  message: string;
  page: number | null;
}

export interface Tag {
  id: string;
  name: string;
  color: string;
}

export interface Bookmark {
  documentId: string;
  note: string;
  createdAt: string;
}

export interface PreviewDocument {
  documentId: string;
  fileName: string;
  path: string;
  extension: string;
  markdown: string;
  blocks: PreviewBlock[];
  warnings: PreviewWarning[];
  bookmarked: boolean;
  bookmarkNote: string;
  tags: Tag[];
  truncated: boolean;
}

export interface IndexStatus {
  jobId: string;
  state: IndexState;
  totalFiles: number;
  completedFiles: number;
  currentPath: string | null;
  errors: IndexFailure[];
}

export type IndexState =
  | "queued"
  | "discovering"
  | "parsing"
  | "paused"
  | "completed"
  | "cancelled"
  | "failed";

export interface IndexFailure {
  code: string;
  fileName: string;
  message: string;
}

export interface AppSettings {
  language: string;
  theme: string;
  historyRetentionDays: number;
  minimizeToTray: boolean;
  startWithWindows: boolean;
  startHidden: boolean;
  maxFileSizeBytes: number;
  resultPageSize: number;
  fileClickBehavior?: "preview" | "open";
  dateDisplay?: "relative" | "absolute";
  excludedPathPatterns?: string[];
  indexingIntensity?: "low" | "balanced" | "high";
}

export interface StatisticsBucket {
  label: string;
  count: string;
}

export interface FolderStatisticsBucket extends StatisticsBucket {
  id: string;
}

export interface StatisticsDocument {
  documentId: string;
  fileName: string;
  path: string;
  extension: string;
  sizeBytes: string;
  modifiedAt: string;
}

export interface SearchHistoryRecord {
  id: string;
  query: string;
  mode: string;
  filters: Record<string, unknown>;
  resultCount: string;
  elapsedMs: string;
  searchedAt: string;
}

export interface SearchFrequency {
  query: string;
  count: string;
  lastSearchedAt: string;
}

export interface DocumentStatistics {
  totalDocuments: string;
  indexedDocuments: string;
  totalBytes: string;
  byExtension: StatisticsBucket[];
  byFolder: FolderStatisticsBucket[];
  byYear: StatisticsBucket[];
  recentlyModified: StatisticsDocument[];
  largestDocuments: StatisticsDocument[];
  parseStates: StatisticsBucket[];
  totalSearches: string;
  uniqueSearchTerms: string;
  frequentSearches: SearchFrequency[];
  recentSearches: SearchHistoryRecord[];
}

export interface ParseErrorRecord {
  documentId: string;
  fileName: string;
  path: string;
  errorCode: string;
}

export type StatisticsSearchFilter =
  | { extensions: string[] }
  | { extensionless: true }
  | { folderIds: string[] };

export type ExportFormat = "csv" | "xlsx" | "markdown";

export type ExportRequest =
  | { kind: "searchResults"; hits: SearchHit[] }
  | { kind: "markdownDocument"; fileName: string; markdown: string };

export type ExportOutcome = "written" | "cancelled";
