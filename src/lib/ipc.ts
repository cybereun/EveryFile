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
} from "./types";

export const getSettings = () => invoke<AppSettings>("get_settings");

export const saveSettings = (settings: AppSettings) =>
  invoke<AppSettings>("save_settings", { settings });

export const listFolders = () => invoke<FolderRecord[]>("list_folders");

export const searchDocuments = (request: SearchRequest) =>
  invoke<SearchResponse>("search_documents", { request });

export const cancelSearch = (requestId: string) =>
  invoke<boolean>("cancel_search", { requestId });

export const openSourceFile = (documentId: string) =>
  invoke<void>("open_source_file", { documentId });

export const getPreview = (documentId: string) =>
  invoke<PreviewDocument>("get_preview", { documentId });

export const getPdfBytes = (documentId: string) =>
  invoke<ArrayBuffer>("get_pdf_bytes", { documentId });

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
