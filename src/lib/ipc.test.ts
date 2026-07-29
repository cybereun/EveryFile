import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import {
  cancelIndexing,
  getIndexStatus,
  getPreview,
  getSettings,
  listFolders,
  pauseIndexing,
  resumeIndexing,
  saveSettings,
  searchDocuments,
  startIndexing,
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
      query: "검색어",
      mode: "keyword",
      folderIds: ["folder-1"],
      extensions: ["hwp"],
      modifiedAfter: null,
      modifiedBefore: null,
      includeFilename: true,
      privateSearch: false,
      sort: "relevance",
      limit: 100,
      offset: 0,
    };

    await getSettings();
    await saveSettings(settings);
    await listFolders();
    await searchDocuments(request);
    await getPreview("document-1");
    await startIndexing("folder-1");
    await pauseIndexing("job-1");
    await resumeIndexing("job-1");
    await cancelIndexing("job-1");
    await getIndexStatus("job-1");

    expect(invoke.mock.calls).toEqual([
      ["get_settings"],
      ["save_settings", { settings }],
      ["list_folders"],
      ["search_documents", { request }],
      ["get_preview", { documentId: "document-1" }],
      ["start_indexing", { folderId: "folder-1" }],
      ["pause_indexing", { jobId: "job-1" }],
      ["resume_indexing", { jobId: "job-1" }],
      ["cancel_indexing", { jobId: "job-1" }],
      ["get_index_status", { jobId: "job-1" }],
    ]);
  });
});
