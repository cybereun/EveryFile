import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  FolderRecord,
  IndexStatus,
  Bookmark,
  PreviewDocument,
  SearchRequest,
  SearchResponse,
  Tag,
  DocumentStatistics,
  ExportFormat,
  ExportOutcome,
  ExportRequest,
  ParseErrorRecord,
  SearchHistoryRecord,
} from "./types";

export const getSettings = () => invoke<AppSettings>("get_settings");

export const saveSettings = (settings: AppSettings) =>
  invoke<AppSettings>("save_settings", { settings });

export const getAiSecretStatus = (provider: string) =>
  invoke<boolean>("get_ai_secret_status", { provider });

export const saveAiSecret = (provider: string, secret: string | null) =>
  invoke<void>("save_ai_secret", { provider, secret });

export const runDocumentAi = (
  requestId: string,
  documentId: string,
  question: string | null,
  remoteConsent: boolean,
) =>
  invoke<string>("run_document_ai", {
    requestId,
    documentId,
    question,
    remoteConsent,
  });

export const cancelDocumentAi = (requestId: string) =>
  invoke<boolean>("cancel_document_ai", { requestId });

export const listFolders = () => invoke<FolderRecord[]>("list_folders");

export const registerFolder = () =>
  invoke<FolderRecord | null>("register_folder");

export const removeFolder = (folderId: string) =>
  invoke<void>("remove_folder", { folderId });

export const searchDocuments = (request: SearchRequest) =>
  invoke<SearchResponse>("search_documents", { request });

export const cancelSearch = (requestId: string) =>
  invoke<boolean>("cancel_search", { requestId });

export const openSourceFile = (documentId: string) =>
  invoke<void>("open_source_file", { documentId });

export const getPreview = (documentId: string) =>
  invoke<PreviewDocument>("get_preview", { documentId });

export const getPdfBytes = (documentId: string, requestId: string) =>
  invoke<ArrayBuffer>("get_pdf_bytes", { documentId, requestId });

export const cancelPdfRead = (requestId: string) =>
  invoke<boolean>("cancel_pdf_read", { requestId });

export const openSourceLocation = (documentId: string) =>
  invoke<void>("open_source_location", { documentId });

export const setBookmark = (documentId: string, note: string) =>
  invoke<Bookmark>("set_bookmark", { documentId, note });

export const removeBookmark = (documentId: string) =>
  invoke<void>("remove_bookmark", { documentId });

export const createTag = (name: string, color: string) =>
  invoke<Tag>("create_tag", { name, color });

export const setDocumentTags = (documentId: string, tagIds: string[]) =>
  invoke<Tag[]>("set_document_tags", { documentId, tagIds });

export const saveMarkdown = (documentId: string) =>
  invoke<boolean>("save_markdown", { documentId });

export const startIndexing = (folderId: string) =>
  invoke<string>("start_indexing", { folderId });

export const pauseIndexing = (jobId: string) =>
  invoke<void>("pause_indexing", { jobId });

export const resumeIndexing = (jobId: string) =>
  invoke<void>("resume_indexing", { jobId });

export const cancelIndexing = (jobId: string) =>
  invoke<void>("cancel_indexing", { jobId });

export const getIndexStatus = (jobId: string) =>
  invoke<IndexStatus>("get_index_status", { jobId });

export const getStatistics = () =>
  invoke<DocumentStatistics>("get_statistics");

export const listSearchHistory = (limit = 100, offset = 0) =>
  invoke<SearchHistoryRecord[]>("list_search_history", { limit, offset });

export const deleteSearchHistory = (id: string) =>
  invoke<void>("delete_search_history", { id });

export const clearSearchHistory = () =>
  invoke<void>("clear_search_history");

export const exportResults = (request: ExportRequest, format: ExportFormat) =>
  invoke<ExportOutcome>("export_results", { request, format });

export const listParseErrors = () =>
  invoke<ParseErrorRecord[]>("list_parse_errors");

export const getDiagnosticsLogFolder = () =>
  invoke<string>("get_diagnostics_log_folder");

export const retryParse = (documentId: string) =>
  invoke<void>("retry_parse", { documentId });

export const resetApplicationData = () =>
  invoke<void>("reset_application_data", { confirmed: true });
