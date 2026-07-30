import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { IndexStatus } from "../../lib/types";
import { IndexStatusController } from "./IndexStatus";

const bridge = vi.hoisted(() => ({
  listen: vi.fn(),
  unlisten: vi.fn(),
  pause: vi.fn(),
  resume: vi.fn(),
  cancel: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({ listen: bridge.listen }));
vi.mock("../../lib/ipc", () => ({
  pauseIndexing: bridge.pause,
  resumeIndexing: bridge.resume,
  cancelIndexing: bridge.cancel,
}));

describe("IndexStatusController", () => {
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("listens on mount, keeps paths private, routes controls, and cleans up", async () => {
    let receive: ((event: { payload: IndexStatus }) => void) | undefined;
    bridge.listen.mockImplementation(
      (_topic: string, handler: (event: { payload: IndexStatus }) => void) => {
        receive = handler;
        return Promise.resolve(bridge.unlisten);
      },
    );
    const view = render(<IndexStatusController />);
    await waitFor(() =>
      expect(bridge.listen).toHaveBeenCalledWith(
        "index-status://changed",
        expect.any(Function),
      ),
    );

    act(() => {
      receive?.({
        payload: {
          jobId: "job-private",
          state: "parsing",
          totalFiles: 3,
          completedFiles: 1,
          currentPath: "C:\\Users\\me\\Secret\\budget.xlsx",
          errors: [],
        },
      });
    });
    expect(screen.getByText("budget.xlsx")).toBeVisible();
    expect(
      screen.queryByText("C:\\Users\\me\\Secret\\budget.xlsx"),
    ).not.toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", { name: "\uC77C\uC2DC\uC815\uC9C0" }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "\uCDE8\uC18C" }),
    );
    expect(bridge.pause).toHaveBeenCalledWith("job-private");
    expect(bridge.cancel).toHaveBeenCalledWith("job-private");

    act(() => {
      receive?.({
        payload: {
          jobId: "job-private",
          state: "paused",
          totalFiles: 3,
          completedFiles: 1,
          currentPath: null,
          errors: [],
        },
      });
    });
    fireEvent.click(
      screen.getByRole("button", { name: "\uACC4\uC18D" }),
    );
    expect(bridge.resume).toHaveBeenCalledWith("job-private");

    view.unmount();
    expect(bridge.unlisten).toHaveBeenCalledOnce();
  });
});
