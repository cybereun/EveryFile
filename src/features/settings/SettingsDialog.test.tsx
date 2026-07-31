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
  it("offers local OCR and separate math OCR controls in Search", async () => {
    render(
      <SettingsDialog
        open
        onClose={() => undefined}
        loadSettings={async () => settings}
      />,
    );

    fireEvent.click(await screen.findByRole("tab", { name: /Search|검색/ }));
    const localOcr = screen.getByRole("checkbox", { name: "로컬 OCR 활성화" });
    const mathOcr = screen.getByRole("checkbox", { name: "수학 OCR 활성화" });
    expect(localOcr).not.toBeChecked();
    expect(mathOcr).toBeDisabled();

    fireEvent.click(localOcr);
    expect(mathOcr).toBeEnabled();
  });

  afterEach(cleanup);

  it("offers the complete settings tabs and gates AI provider controls", async () => {
    render(
      <SettingsDialog
        open
        onClose={() => undefined}
        loadSettings={async () => settings}
        persistSettings={async (value) => value}
      />,
    );

    expect(await screen.findByRole("dialog", { name: "설정" })).toBeVisible();
    expect(screen.getAllByRole("tab")).toHaveLength(5);
    fireEvent.click(screen.getByRole("tab", { name: "AI" }));
    expect(screen.getByRole("checkbox", { name: "AI 기능 활성화" })).not.toBeChecked();
    expect(screen.queryByLabelText("LLM Provider")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("checkbox", { name: "AI 기능 활성화" }));
    expect(screen.getByLabelText("LLM Provider")).toBeVisible();
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

  it("contains focus in the nested reset modal, cancels with Escape, and restores focus", async () => {
    render(
      <SettingsDialog
        open
        onClose={() => undefined}
        loadSettings={async () => settings}
        persistSettings={async (value) => value}
        resetApplicationData={async () => undefined}
      />,
    );
    fireEvent.click(await screen.findByRole("tab", { name: "시스템" }));
    const resetTrigger = screen.getByRole("button", {
      name: "모든 로컬 데이터 초기화",
    });
    resetTrigger.focus();
    fireEvent.click(resetTrigger);

    const confirmation = screen.getByRole("alertdialog", {
      name: "데이터 초기화 확인",
    });
    const cancel = screen.getByRole("button", { name: "취소" });
    const confirm = screen.getByRole("button", { name: "초기화하고 다시 시작" });
    expect(cancel).toHaveFocus();
    const settingsDialog = document.querySelector(".settings-dialog");
    expect(settingsDialog).toHaveAttribute("inert");

    confirm.focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(cancel).toHaveFocus();
    fireEvent.keyDown(document, { key: "Escape" });

    expect(confirmation).not.toBeInTheDocument();
    await waitFor(() => expect(resetTrigger).toHaveFocus());
    expect(settingsDialog).not.toHaveAttribute("inert");
  });

  it("keeps the confirmation open and reports a rejected reset", async () => {
    render(
      <SettingsDialog
        open
        onClose={() => undefined}
        loadSettings={async () => settings}
        persistSettings={async (value) => value}
        resetApplicationData={async () => {
          throw new Error("Reset worker could not start");
        }}
      />,
    );
    fireEvent.click(await screen.findByRole("tab", { name: "시스템" }));
    fireEvent.click(screen.getByRole("button", { name: "모든 로컬 데이터 초기화" }));
    fireEvent.click(screen.getByRole("button", { name: "초기화하고 다시 시작" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Reset worker could not start",
    );
    expect(
      screen.getByRole("alertdialog", { name: "데이터 초기화 확인" }),
    ).toBeVisible();
  });
});
