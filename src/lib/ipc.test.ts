import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import {
  cancelSearch,
  cancelIndexing,
  cancelPdfRead,
  createTag,
  getPdfBytes,
  getIndexStatus,
  getAiSecretStatus,
  getPreview,
  getSettings,
  listFolders,
  registerFolder,
  removeFolder,
  openSourceFile,
  openSourceLocation,
  pauseIndexing,
  resumeIndexing,
  runDocumentAi,
  removeBookmark,
  saveMarkdown,
  saveAiSecret,
  saveSettings,
  searchDocuments,
  startIndexing,
  setBookmark,
  setDocumentTags,
} from "./ipc";
import type { AppSettings, SearchRequest } from "./types";

describe("IPC wrappers", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("uses each stable command name and camelCase payload", async () => {
    const settings: AppSettings = {
      language: "ko",
      theme: "light",
      historyRetentionDays: 90,
      minimizeToTray: false,
      startWithWindows: false,
      startHidden: false,
      maxFileSizeBytes: 200 * 1024 * 1024,
      resultPageSize: 100,
    };
    const request: SearchRequest = {
      requestId: "search-1",
      query: "검색어",
      mode: "keyword",
      folderIds: ["folder-1"],
      extensions: ["hwp"],
      modifiedAfter: null,
      modifiedBefore: null,
      includeFilename: true,
      termMode: "all",
      privateSearch: false,
      sort: "relevance",
      limit: 100,
      offset: 0,
    };

    await getSettings();
    await saveSettings(settings);
    await getAiSecretStatus("openai");
    await saveAiSecret("openai", "secret");
    await runDocumentAi("document-1", "질문", true);
    await listFolders();
    await registerFolder();
    await removeFolder("folder-1");
    await searchDocuments(request);
    await cancelSearch("search-1");
    await openSourceFile("document-1");
    await openSourceLocation("document-1");
    await getPreview("document-1");
    await getPdfBytes("document-1", "pdf-1");
    await cancelPdfRead("pdf-1");
    await setBookmark("document-1", "note");
    await removeBookmark("document-1");
    await createTag("Work", "terracotta");
    await setDocumentTags("document-1", ["tag-1"]);
    await saveMarkdown("document-1");
    await startIndexing("folder-1");
    await pauseIndexing("job-1");
    await resumeIndexing("job-1");
    await cancelIndexing("job-1");
    await getIndexStatus("job-1");

    expect(invoke.mock.calls).toEqual([
      ["get_settings"],
      ["save_settings", { settings }],
      ["get_ai_secret_status", { provider: "openai" }],
      ["save_ai_secret", { provider: "openai", secret: "secret" }],
      [
        "run_document_ai",
        { documentId: "document-1", question: "질문", remoteConsent: true },
      ],
      ["list_folders"],
      ["register_folder"],
      ["remove_folder", { folderId: "folder-1" }],
      ["search_documents", { request }],
      ["cancel_search", { requestId: "search-1" }],
      ["open_source_file", { documentId: "document-1" }],
      ["open_source_location", { documentId: "document-1" }],
      ["get_preview", { documentId: "document-1" }],
      ["get_pdf_bytes", { documentId: "document-1", requestId: "pdf-1" }],
      ["cancel_pdf_read", { requestId: "pdf-1" }],
      ["set_bookmark", { documentId: "document-1", note: "note" }],
      ["remove_bookmark", { documentId: "document-1" }],
      ["create_tag", { name: "Work", color: "terracotta" }],
      ["set_document_tags", { documentId: "document-1", tagIds: ["tag-1"] }],
      ["save_markdown", { documentId: "document-1" }],
      ["start_indexing", { folderId: "folder-1" }],
      ["pause_indexing", { jobId: "job-1" }],
      ["resume_indexing", { jobId: "job-1" }],
      ["cancel_indexing", { jobId: "job-1" }],
      ["get_index_status", { jobId: "job-1" }],
    ]);
  });
});
