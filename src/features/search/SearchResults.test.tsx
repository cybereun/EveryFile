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

  it("preserves backend order and selects by document identity across replacements", async () => {
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
    await waitFor(() => expect(open).toHaveBeenCalledWith("new-first"));
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
});
