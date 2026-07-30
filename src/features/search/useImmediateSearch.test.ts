import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { SearchRequest, SearchResponse } from "../../lib/types";
import {
  DEFAULT_SEARCH_FILTERS,
  buildBackendQuery,
  type SearchFilters,
} from "./searchStore";
import { useImmediateSearch } from "./useImmediateSearch";

function response(
  requestId: string,
  fileName: string,
  options: Partial<SearchResponse> = {},
): SearchResponse {
  return {
    requestId,
    hits: [
      {
        documentId: fileName,
        fileName,
        path: `C:\\Documents\\${fileName}`,
        extension: "hwp",
        sizeBytes: 1024,
        modifiedAt: "2026-07-30T00:00:00Z",
        snippet: `<mark>${fileName}</mark>`,
        score: 1,
      },
    ],
    total: 1,
    elapsedMs: 2,
    appliedFilters: [],
    hasMore: false,
    ...options,
  };
}

describe("useImmediateSearch", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("keeps only the newest response and cancels the previous Rust search", async () => {
    vi.useFakeTimers();
    const pending = new Map<
      string,
      (value: SearchResponse) => void
    >();
    const search = vi.fn(
      (request: SearchRequest) =>
        new Promise<SearchResponse>((resolve) => {
          pending.set(request.query, resolve);
        }),
    );
    const cancel = vi.fn().mockResolvedValue(true);
    const { result } = renderHook(() =>
      useImmediateSearch({ search, cancel }),
    );

    act(() => result.current.setQuery("중간"));
    await act(async () => vi.advanceTimersByTimeAsync(120));
    const firstRequestId = search.mock.calls[0][0].requestId;

    act(() => result.current.setQuery("중간고사"));
    await act(async () => vi.advanceTimersByTimeAsync(120));
    const secondRequestId = search.mock.calls[1][0].requestId;

    expect(cancel).toHaveBeenCalledWith(firstRequestId);
    await act(async () => {
      pending.get("중간고사")?.(response(secondRequestId, "latest.hwp"));
    });
    await act(async () => {
      pending.get("중간")?.(response(firstRequestId, "stale.hwp"));
    });

    expect(result.current.hits[0]?.fileName).toBe("latest.hwp");
  });

  it("debounces by 120ms and maps filters into a structured request", async () => {
    vi.useFakeTimers();
    const search = vi.fn(async (request: SearchRequest) =>
      response(request.requestId, "result.hwp"),
    );
    const cancel = vi.fn().mockResolvedValue(false);
    const filters: SearchFilters = {
      ...DEFAULT_SEARCH_FILTERS,
      mode: "filename",
      extensions: ["hwp", "pdf"],
      modifiedAfter: "2026-07-01",
      modifiedBefore: "2026-07-30",
      folderIds: ["documents"],
      includeFilename: false,
      sort: "newest",
    };
    const { result } = renderHook(() =>
      useImmediateSearch({
        search,
        cancel,
        initialFilters: filters,
      }),
    );

    act(() => result.current.setQuery("report"));
    await act(async () => vi.advanceTimersByTimeAsync(119));
    expect(search).not.toHaveBeenCalled();
    await act(async () => vi.advanceTimersByTimeAsync(1));

    expect(search).toHaveBeenCalledWith(
      expect.objectContaining({
        query: "report",
        mode: "filename",
        extensions: ["hwp", "pdf"],
        modifiedAfter: "2026-07-01",
        modifiedBefore: "2026-07-30",
        folderIds: ["documents"],
        includeFilename: false,
        sort: "newest",
      }),
    );
  });

  it("loads the next page without replacing the first page", async () => {
    const search = vi.fn(async (request: SearchRequest) =>
      response(
        request.requestId,
        request.offset === 0 ? "first.hwp" : "second.hwp",
        { hasMore: request.offset === 0, total: 2 },
      ),
    );
    const cancel = vi.fn().mockResolvedValue(false);
    const { result } = renderHook(() =>
      useImmediateSearch({
        search,
        cancel,
        debounceMs: 0,
        pageSize: 1,
      }),
    );

    act(() => result.current.setQuery("report"));
    await waitFor(() => expect(result.current.hits).toHaveLength(1));
    await act(async () => result.current.loadMore());

    expect(result.current.hits.map((hit) => hit.fileName)).toEqual([
      "first.hwp",
      "second.hwp",
    ]);
  });

  it("maps every visible term option without disturbing query operators", () => {
    expect(buildBackendQuery("alpha beta ext:pdf", "all")).toBe(
      "alpha beta ext:pdf",
    );
    expect(buildBackendQuery("alpha beta ext:pdf", "any")).toBe(
      "alpha OR beta ext:pdf",
    );
    expect(buildBackendQuery("alpha beta ext:pdf", "exact")).toBe(
      '"alpha beta" ext:pdf',
    );
    expect(buildBackendQuery("alpha beta ext:pdf", "exclude")).toBe(
      "-alpha -beta ext:pdf",
    );
    expect(buildBackendQuery("alpha beta ext:pdf", "near")).toBe(
      "alpha beta ~5 ext:pdf",
    );
  });
});
