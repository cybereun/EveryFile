import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AppProps } from "./App";

const mocks = vi.hoisted(() => ({
  listFolders: vi.fn(),
  registerFolder: vi.fn(),
  removeFolder: vi.fn(),
  openFolderLocation: vi.fn(),
  startIndexing: vi.fn(),
  getIndexStatus: vi.fn(),
  listen: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));
vi.mock("../lib/ipc", () => ({
  listFolders: mocks.listFolders,
  registerFolder: mocks.registerFolder,
  removeFolder: mocks.removeFolder,
  openFolderLocation: mocks.openFolderLocation,
  startIndexing: mocks.startIndexing,
  getIndexStatus: mocks.getIndexStatus,
}));
vi.mock("./App", () => ({
  App: (props: AppProps) => (
    <div>
      <span data-testid="folder-count">{props.folders?.length ?? 0}</span>
      <span data-testid="queue-state">{props.queueState}</span>
      <button type="button" onClick={props.onAddFolder}>
        add
      </button>
      <span>{props.commandStatus?.text}</span>
    </div>
  ),
}));

import { DesktopApp } from "./DesktopApp";

const folder = {
  id: "folder-1",
  canonicalPath: "C:\\Fixture",
  displayName: "Fixture",
  documentCount: 0,
  indexState: "registered",
};

describe("DesktopApp", () => {
  beforeEach(() => {
    mocks.listFolders.mockReset().mockResolvedValue([]);
    mocks.registerFolder.mockReset().mockResolvedValue({ folder, jobId: "job-1" });
    mocks.removeFolder.mockReset().mockResolvedValue(undefined);
    mocks.openFolderLocation.mockReset().mockResolvedValue(undefined);
    mocks.startIndexing.mockReset().mockResolvedValue("job-1");
    mocks.getIndexStatus.mockReset().mockResolvedValue({
      jobId: "job-1",
      state: "parsing",
      totalFiles: 1,
      completedFiles: 0,
      currentPath: null,
      errorCount: 0,
      errors: [],
    });
    mocks.listen.mockReset().mockResolvedValue(() => undefined);
  });

  afterEach(cleanup);

  it("loads folders and registers with the single backend-started indexing job", async () => {
    mocks.listFolders
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([folder]);
    render(<DesktopApp />);

    await waitFor(() => expect(mocks.listFolders).toHaveBeenCalledOnce());
    fireEvent.click(screen.getByRole("button", { name: "add" }));

    await waitFor(() => expect(mocks.registerFolder).toHaveBeenCalledOnce());
    expect(mocks.startIndexing).not.toHaveBeenCalled();
    expect(screen.getByTestId("folder-count")).toHaveTextContent("1");
    expect(screen.getByTestId("queue-state")).toHaveTextContent("indexing");
  });
});
