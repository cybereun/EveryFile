import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";

describe("App", () => {
  beforeEach(() => {
    Object.defineProperty(window, "innerWidth", {
      configurable: true,
      value: 1440,
      writable: true,
    });
  });

  afterEach(() => {
    cleanup();
    window.localStorage.clear();
  });

  it("renders the Korean EveryFile product identity by default", () => {
    render(<App />);

    expect(screen.getByRole("heading", { name: "EveryFile" })).toBeVisible();
    expect(
      screen.getByText("\uD30C\uC77C\uC744 \uCC3E\uB294 \uAC00\uC7A5 \uBE60\uB978 \uBC29\uBC95"),
    ).toBeVisible();
  });

  it("does not render a redundant header language chooser", () => {
    render(<App />);

    expect(screen.queryByRole("combobox", { name: "Language" })).not.toBeInTheDocument();
  });

  it("renders the three-pane shell and status information", () => {
    render(
      <App
        folders={[
          {
            id: "documents",
            canonicalPath: "C:\\Users\\Lebi\\Documents",
            displayName: "Documents",
            documentCount: 42,
            indexState: "completed",
          },
        ]}
        indexedDocumentCount={42}
        queueState="idle"
        selectedDocumentId="document-1"
      />,
    );

    expect(
      screen.getByRole("complementary", { name: "등록 폴더 / Indexed folders" }),
    ).toBeVisible();
    expect(screen.getByRole("search", { name: "파일 검색 / File search" })).toBeVisible();
    expect(
      screen.getByRole("region", { name: "문서 미리보기 / Document preview" }),
    ).toBeVisible();
    const statusSummary = document.querySelector(".status-summary");
    expect(statusSummary).not.toBeNull();
    expect(statusSummary).toHaveTextContent("42");
    expect(statusSummary).toHaveTextContent("1");
    expect(statusSummary).toHaveTextContent("v1.1.1");
  });

  it("removes an indexed folder only through its three-dot menu and confirmation", () => {
    const remove = vi.fn();
    render(
      <App
        folders={[{
          id: "documents",
          canonicalPath: "C:\\Users\\Lebi\\Documents",
          displayName: "Documents",
          documentCount: 42,
          indexState: "completed",
        }]}
        onRemoveFolder={remove}
      />,
    );

    expect(screen.queryByRole("button", { name: "Documents 제거" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Documents 폴더 메뉴" }));
    fireEvent.click(screen.getByRole("menuitem", { name: /폴더 제거/ }));
    const dialog = screen.getByRole("alertdialog", { name: "색인 폴더를 제거할까요?" });
    expect(dialog).toHaveTextContent("원본 파일은 삭제하지 않습니다");
    fireEvent.click(within(dialog).getByRole("button", { name: "폴더 제거" }));
    expect(remove).toHaveBeenCalledWith("documents");
  });

  it("toggles the sidebar with Ctrl+B and focuses search with slash", () => {
    render(<App />);

    fireEvent.keyDown(window, { key: "b", ctrlKey: true });
    expect(
      screen.getByRole("complementary", {
        name: "등록 폴더 / Indexed folders",
        hidden: true,
      }),
    ).not.toBeVisible();

    fireEvent.keyDown(window, { key: "/" });
    expect(screen.getByRole("searchbox")).toHaveFocus();
  });

  it("toggles and persists the left and right panels independently", () => {
    const { unmount } = render(<App selectedDocumentId="document-1" />);

    fireEvent.click(
      screen.getByRole("button", { name: /왼쪽 패널 닫기.*Toggle left panel/ }),
    );
    expect(
      screen.getByRole("complementary", {
        name: /Indexed folders/,
        hidden: true,
      }),
    ).not.toBeVisible();
    expect(
      screen.getByRole("region", {
        name: /Document preview/,
      }),
    ).toBeVisible();

    fireEvent.click(
      screen.getByRole("button", { name: /오른쪽 패널 닫기.*Toggle right panel/ }),
    );
    expect(
      screen.getByRole("region", {
        name: /Document preview/,
        hidden: true,
      }),
    ).not.toBeVisible();
    expect(window.localStorage.getItem("everyfile.ui.left-pane-visible")).toBe(
      "false",
    );
    expect(window.localStorage.getItem("everyfile.ui.right-pane-visible")).toBe(
      "false",
    );

    unmount();
    render(<App selectedDocumentId="document-1" />);
    expect(
      screen.getByRole("complementary", {
        name: /Indexed folders/,
        hidden: true,
      }),
    ).not.toBeVisible();
    expect(
      screen.getByRole("region", {
        name: /Document preview/,
        hidden: true,
      }),
    ).not.toBeVisible();
  });

  it("adds a folder from the empty left panel", () => {
    const onAddFolder = vi.fn();
    render(<App onAddFolder={onAddFolder} />);

    fireEvent.click(screen.getByRole("button", { name: "폴더 추가" }));

    expect(onAddFolder).toHaveBeenCalledOnce();
  });

  it("does not hijack Ctrl+B or slash in text-entry controls", () => {
    render(<App />);
    const controls: HTMLElement[] = [screen.getByRole("searchbox")];
    const textarea = document.createElement("textarea");
    textarea.setAttribute("aria-label", "Test textarea");
    document.body.append(textarea);
    controls.push(textarea);
    const editor = document.createElement("div");
    editor.setAttribute("contenteditable", "true");
    editor.setAttribute("aria-label", "Test editor");
    editor.tabIndex = 0;
    document.body.append(editor);
    controls.push(editor);

    for (const control of controls) {
      control.focus();
      fireEvent.keyDown(control, { key: "b", ctrlKey: true });
      expect(
        screen.getByRole("complementary", {
          name: "등록 폴더 / Indexed folders",
        }),
      ).toBeVisible();

      fireEvent.keyDown(control, { key: "/" });
      expect(control).toHaveFocus();
    }

    textarea.remove();
    editor.remove();
  });

  it("persists resized pane widths in local settings", () => {
    const { unmount } = render(<App />);
    const leftSeparator = screen.getByRole("separator", {
      name: "폴더 패널 크기 조절 / Resize folder pane",
    });

    fireEvent.keyDown(leftSeparator, { key: "ArrowRight" });
    expect(window.localStorage.getItem("everyfile.ui.left-pane-width")).toBe("276");

    unmount();
    render(<App />);
    expect(
      screen.getByRole("separator", {
        name: "폴더 패널 크기 조절 / Resize folder pane",
      }),
    ).toHaveAttribute("aria-valuenow", "276");
  });

  it("collapses the preview before removing search at narrow widths", () => {
    render(<App selectedDocumentId="document-1" />);
    window.innerWidth = 400;
    fireEvent(window, new Event("resize"));

    expect(
      screen.getByRole("region", {
        name: "문서 미리보기 / Document preview",
        hidden: true,
      }),
    ).not.toBeVisible();
    expect(screen.getByRole("searchbox")).toBeVisible();
    expect(screen.getByRole("button", { name: "더보기 / More" })).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "더보기 / More" }));
    expect(screen.getByRole("button", { name: "통계 / Statistics" })).toBeVisible();
    expect(screen.getByRole("button", { name: "폴더 추가 / Add folder" })).toBeVisible();
    expect(screen.getByRole("button", { name: "설정 / Settings" })).toBeVisible();
  });

  it("keeps a 520px center by constraining combined persisted pane widths", () => {
    window.localStorage.setItem("everyfile.ui.left-pane-width", "420");
    window.localStorage.setItem("everyfile.ui.right-pane-width", "720");
    render(<App />);

    expect(
      screen.getByRole("separator", {
        name: "폴더 패널 크기 조절 / Resize folder pane",
      }),
    ).toHaveAttribute("aria-valuenow", "420");
    expect(
      screen.getByRole("separator", {
        name: "미리보기 패널 크기 조절 / Resize preview pane",
      }),
    ).toHaveAttribute("aria-valuenow", "500");

    window.innerWidth = 1101;
    fireEvent(window, new Event("resize"));
    expect(
      screen.getByRole("separator", {
        name: "폴더 패널 크기 조절 / Resize folder pane",
      }),
    ).toHaveAttribute("aria-valuenow", "301");
    expect(
      screen.getByRole("separator", {
        name: "미리보기 패널 크기 조절 / Resize preview pane",
      }),
    ).toHaveAttribute("aria-valuenow", "280");

    window.innerWidth = 1100;
    fireEvent(window, new Event("resize"));
    expect(
      screen.getByRole("region", {
        name: "문서 미리보기 / Document preview",
        hidden: true,
      }),
    ).not.toBeVisible();
    expect(screen.getByRole("searchbox")).toBeVisible();
  });

  it("falls back from corrupt persisted widths", () => {
    window.localStorage.setItem("everyfile.ui.left-pane-width", "-1");
    window.localStorage.setItem("everyfile.ui.right-pane-width", "Infinity");
    render(<App />);

    expect(
      screen.getByRole("separator", {
        name: "폴더 패널 크기 조절 / Resize folder pane",
      }),
    ).toHaveAttribute("aria-valuenow", "260");
    expect(
      screen.getByRole("separator", {
        name: "미리보기 패널 크기 조절 / Resize preview pane",
      }),
    ).toHaveAttribute("aria-valuenow", "448");
  });
});
