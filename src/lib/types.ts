export type SearchMode = "keyword" | "filename";
export type SearchTermMode = "all" | "any" | "exact" | "exclude" | "near";
export type SearchMatchKind = "filename" | "content" | "both" | "metadata";

export interface SearchRequest {
  requestId: string;
  query: string;
  mode: SearchMode;
  folderIds: string[];
  extensions: string[];
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
}
