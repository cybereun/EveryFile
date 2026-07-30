import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AppSettings } from "../../lib/types";
import { SettingsDialog } from "./SettingsDialog";

const settings: AppSettings = {
  language: "ko",
  theme: "light",
  historyRetentionDays: 90,
  minimizeToTray: false,
  startWithWindows: false,
  startHidden: false,
  maxFileSizeBytes: 209_715_200,
  resultPageSize: 100,
};

describe("SettingsDialog", () => {
  afterEach(cleanup);

  it("offers the four Phase 1 tabs without AI controls", async () => {
    render(
      <SettingsDialog
        open
        onClose={() => undefined}
        loadSettings={async () => settings}
        persistSettings={async (value) => value}
      />,
    );

    expect(await screen.findByRole("dialog", { name: "설정" })).toBeVisible();
    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual([
      "일반",
      "검색",
      "시스템",
      "진단",
    ]);
    expect(screen.queryByText(/AI 기능|LLM Provider/i)).not.toBeInTheDocument();
  });

  it("saves retention changes and restores focus after Escape", async () => {
    const save = vi.fn(async (value: AppSettings) => value);
    const trigger = document.createElement("button");
    trigger.textContent = "trigger";
    document.body.append(trigger);
    trigger.focus();

    render(
      <SettingsDialog
        open
        onClose={() => undefined}
        loadSettings={async () => settings}
        persistSettings={save}
      />,
    );
    fireEvent.click(await screen.findByRole("tab", { name: "검색" }));
    fireEvent.change(screen.getByLabelText("검색 히스토리 보관 기간"), {
      target: { value: "365" },
    });
    fireEvent.click(screen.getByRole("button", { name: "저장" }));

    await waitFor(() =>
      expect(save).toHaveBeenCalledWith(
        expect.objectContaining({ historyRetentionDays: 365 }),
      ),
    );
    fireEvent.keyDown(document, { key: "Escape" });
    expect(trigger).toHaveFocus();
    trigger.remove();
  });

  it("requires an explicit destructive confirmation before reset", async () => {
    const reset = vi.fn(async () => undefined);
    render(
      <SettingsDialog
        open
        onClose={() => undefined}
        loadSettings={async () => settings}
        persistSettings={async (value) => value}
        resetApplicationData={reset}
      />,
    );
    fireEvent.click(await screen.findByRole("tab", { name: "시스템" }));
    fireEvent.click(screen.getByRole("button", { name: "모든 로컬 데이터 초기화" }));

    expect(reset).not.toHaveBeenCalled();
    expect(screen.getByRole("alertdialog", { name: "데이터 초기화 확인" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "초기화하고 다시 시작" }));
    await waitFor(() => expect(reset).toHaveBeenCalledOnce());
  });
});
