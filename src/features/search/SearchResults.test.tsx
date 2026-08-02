import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { SearchHit } from "../../lib/types";
import { SearchResults } from "./SearchResults";

function hit(documentId: string, matchKind: SearchHit["matchKind"]): SearchHit {
  return {
    documentId,
    fileName: `${documentId}.pdf`,
    path: `C:\\Documents\\${documentId}.pdf`,
    extension: "pdf",
    sizeBytes: 10,
    modifiedAt: "2026-07-30T00:00:00Z",
    snippet: null,
    score: 1,
    matchKind,
  };
}

describe("SearchResults", () => {
  afterEach(cleanup);

  it("groups matches by kind and selects by document identity across replacements", async () => {
    const open = vi.fn().mockResolvedValue(undefined);
    const select = vi.fn();
    const props = {
      total: 2,
      elapsedMs: 1,
      loading: false,
      error: null,
      hasMore: false,
      onLoadMore: vi.fn(),
      onOpen: open,
      onSelect: select,
    };
    const { rerender } = render(
      <SearchResults
        {...props}
        hits={[hit("content-first", "content"), hit("filename-second", "filename")]}
      />,
    );

    expect(
      screen.getAllByRole("option").map((option) => option.textContent),
    ).toEqual([
      expect.stringContaining("content-first.pdf"),
      expect.stringContaining("filename-second.pdf"),
    ]);
    fireEvent.click(screen.getAllByRole("option")[1]);

    rerender(
      <SearchResults
        {...props}
        hits={[hit("new-first", "filename"), hit("new-second", "content")]}
      />,
    );
    await waitFor(() =>
      expect(screen.getAllByRole("option")[0]).toHaveAttribute("aria-selected", "true"),
    );
    fireEvent.keyDown(screen.getByRole("listbox"), { key: "Enter" });
    await waitFor(() => expect(open).toHaveBeenCalledWith("new-second"));
  });

  it("reports double-click open failures through the same guarded path", async () => {
    render(
      <SearchResults
        hits={[hit("broken", "filename")]}
        total={1}
        elapsedMs={1}
        loading={false}
        error={null}
        hasMore={false}
        onLoadMore={vi.fn()}
        onOpen={vi.fn().mockRejectedValue(new Error("cannot open"))}
        onSelect={vi.fn()}
      />,
    );

    fireEvent.doubleClick(screen.getByRole("option"));

    expect(await screen.findByRole("alert")).toHaveTextContent("cannot open");
  });

  it("formats legacy Unix-nanosecond modification times", () => {
    render(
      <SearchResults
        hits={[{ ...hit("legacy", "filename"), modifiedAt: "1756473860000000000" }]}
        total={1}
        elapsedMs={1}
        loading={false}
        error={null}
        hasMore={false}
        onLoadMore={vi.fn()}
        onOpen={vi.fn().mockResolvedValue(undefined)}
        onSelect={vi.fn()}
        dateDisplay="absolute"
      />,
    );

    expect(screen.getByRole("option")).toHaveTextContent("2025. 8. 29.");
    expect(screen.getByRole("option")).not.toHaveTextContent("1756473860000000000");
  });

  it("opens result actions and the context menu for the selected file", async () => {
    const open = vi.fn().mockResolvedValue(undefined);
    const openLocation = vi.fn().mockResolvedValue(undefined);
    const clipboard = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: clipboard },
    });

    render(
      <SearchResults
        hits={[hit("action", "both")]}
        total={1}
        elapsedMs={1}
        loading={false}
        error={null}
        hasMore={false}
        onLoadMore={vi.fn()}
        onOpen={open}
        onOpenLocation={openLocation}
        onSelect={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "action.pdf 파일 위치 열기" }));
    await waitFor(() => expect(openLocation).toHaveBeenCalledWith("action"));
    fireEvent.contextMenu(screen.getByRole("option"), { clientX: 80, clientY: 80 });
    expect(screen.getByRole("menu", { name: "검색 결과 메뉴" })).toBeVisible();
    fireEvent.click(screen.getByRole("menuitem", { name: /경로 복사/ }));
    await waitFor(() => expect(clipboard).toHaveBeenCalledWith("C:\\Documents\\action.pdf"));
    fireEvent.contextMenu(screen.getByRole("option"), { clientX: 80, clientY: 80 });
    fireEvent.click(screen.getByRole("menuitem", { name: "비교 대상으로 선택" }));
    expect(screen.getByRole("option")).toHaveClass("is-compare-target");
    expect(open).not.toHaveBeenCalled();
  });
});
