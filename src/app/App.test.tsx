import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "./App";

describe("App", () => {
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

  it("renders the English identity after language selection", () => {
    render(<App />);

    fireEvent.change(screen.getByRole("combobox", { name: "Language" }), {
      target: { value: "en" },
    });

    expect(screen.getByText("The fastest way to find files.")).toBeVisible();
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
    expect(screen.getByRole("status")).toHaveTextContent("42");
    expect(screen.getByRole("status")).toHaveTextContent("1");
    expect(screen.getByRole("status")).toHaveTextContent("v0.1.0");
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
    const originalMatchMedia = window.matchMedia;
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      value: () => ({
        matches: false,
        media: "(min-width: 1101px)",
        onchange: null,
        addEventListener: () => undefined,
        removeEventListener: () => undefined,
        addListener: () => undefined,
        removeListener: () => undefined,
        dispatchEvent: () => true,
      }),
    });

    try {
      render(<App selectedDocumentId="document-1" />);
      expect(
        screen.getByRole("region", {
          name: "문서 미리보기 / Document preview",
          hidden: true,
        }),
      ).not.toBeVisible();
      expect(screen.getByRole("searchbox")).toBeVisible();
    } finally {
      Object.defineProperty(window, "matchMedia", {
        configurable: true,
        value: originalMatchMedia,
      });
    }
  });
});
