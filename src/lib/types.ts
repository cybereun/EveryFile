export type SearchMode = "keyword" | "filename";

export interface SearchRequest {
  query: string;
  mode: SearchMode;
  folderIds: string[];
  extensions: string[];
  modifiedAfter: string | null;
  modifiedBefore: string | null;
  includeFilename: boolean;
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
  hits: SearchHit[];
  total: number;
  elapsedMs: number;
  appliedFilters: string[];
  hasMore: boolean;
}

export interface PreviewBlock {
  kind: string;
  text: string;
  level: number | null;
  pageNumber: number | null;
}

export interface PreviewDocument {
  documentId: string;
  fileName: string;
  path: string;
  extension: string;
  markdown: string;
  blocks: PreviewBlock[];
  warnings: string[];
  bookmarked: boolean;
  tags: string[];
}

export interface IndexStatus {
  jobId: string;
  state: string;
  totalFiles: number;
  completedFiles: number;
  currentPath: string | null;
  errors: IndexFailure[];
}

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
