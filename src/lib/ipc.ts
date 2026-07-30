import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  FolderRecord,
  IndexStatus,
  PreviewDocument,
  SearchRequest,
  SearchResponse,
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
