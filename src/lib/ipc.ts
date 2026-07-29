import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  FolderRecord,
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

export const getPreview = (documentId: string) =>
  invoke<PreviewDocument>("get_preview", { documentId });
