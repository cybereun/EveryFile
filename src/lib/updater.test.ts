import { beforeEach, describe, expect, it, vi } from "vitest";
import { checkForUpdate, type UpdateProgress } from "./updater";

const mocks = vi.hoisted(() => ({ check: vi.fn(), download: vi.fn(), install: vi.fn(), relaunch: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: mocks.check }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: mocks.relaunch }));

beforeEach(() => {
  vi.resetAllMocks();
  mocks.check.mockResolvedValue({ currentVersion: "1.0.4", version: "1.1.0", body: "UI 개선", download: mocks.download, install: mocks.install });
});

describe("signed update handoff", () => {
  it("retains the download size and installs only after verification resolves", async () => {
    const events: UpdateProgress[] = [];
    mocks.download.mockImplementation(async (emit) => {
      emit({ event: "Started", data: { contentLength: 1000 } });
      emit({ event: "Progress", data: { chunkLength: 250 } });
      emit({ event: "Progress", data: { chunkLength: 750 } });
      emit({ event: "Finished" });
      expect(mocks.install).not.toHaveBeenCalled();
    });
    await (await checkForUpdate())!.install(p => events.push(p));
    expect(events.map(p => p.phase)).toEqual(["starting", "downloading", "downloading", "verifying", "installing", "restarting"]);
    expect(events[1]).toMatchObject({ percent: 25, contentLength: 1000 });
    expect(events[3].percent).toBeUndefined();
    expect(mocks.install).toHaveBeenCalledOnce();
    expect(mocks.relaunch).toHaveBeenCalledOnce();
  });

  it("does not invent a percentage for an unknown content length", async () => {
    const events: UpdateProgress[] = [];
    mocks.download.mockImplementation(async emit => {
      emit({ event: "Started", data: {} });
      emit({ event: "Progress", data: { chunkLength: 100 } });
    });
    await (await checkForUpdate())!.install(p => events.push(p));
    expect(events[1].percent).toBeUndefined();
  });

  it("does not exit or install on a bad signature, even after download finished", async () => {
    mocks.download.mockImplementation(async emit => {
      emit({ event: "Finished" });
      throw new Error("signature invalid");
    });
    await expect((await checkForUpdate())!.install()).rejects.toThrow("signature invalid");
    expect(mocks.install).not.toHaveBeenCalled();
    expect(mocks.relaunch).not.toHaveBeenCalled();
  });

  it("leaves installation failures visible and never relaunches the old binary", async () => {
    mocks.install.mockRejectedValue(new Error("access denied"));
    await expect((await checkForUpdate())!.install()).rejects.toThrow("access denied");
    expect(mocks.relaunch).not.toHaveBeenCalled();
  });
});
