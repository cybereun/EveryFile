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
        matchKind: "content",
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
  it("resets paging and cancels stale work when refinement changes", async () => {
    vi.useFakeTimers();
    const pending: Array<{
      request: SearchRequest;
      resolve: (value: SearchResponse) => void;
    }> = [];
    const search = vi.fn(
      (request: SearchRequest) =>
        new Promise<SearchResponse>((resolve) => pending.push({ request, resolve })),
    );
    const cancel = vi.fn().mockResolvedValue(true);
    const { result, rerender } = renderHook(
      ({ withinQuery }) => useImmediateSearch({ search, cancel, withinQuery }),
      { initialProps: { withinQuery: "" } },
    );
    act(() => result.current.setQuery("alpha"));
    await act(async () => vi.advanceTimersByTimeAsync(120));
    await act(async () =>
      pending[0].resolve(
        response(pending[0].request.requestId, "first", {
          total: 200,
          hasMore: true,
        }),
      ),
    );
    act(() => {
      void result.current.loadMore();
    });
    expect(pending[1].request.offset).toBe(1);
    rerender({ withinQuery: "hidden" });
    expect(result.current.hits).toEqual([]);
    expect(result.current.hasMore).toBe(false);
    expect(cancel).toHaveBeenCalledWith(pending[1].request.requestId);
    await act(async () => vi.advanceTimersByTimeAsync(120));
    expect(pending[2].request).toMatchObject({
      offset: 0,
      withinQuery: "hidden",
      query: "alpha",
    });
    await act(async () =>
      pending[2].resolve(response(pending[2].request.requestId, "refined")),
    );
    await act(async () =>
      pending[1].resolve(response(pending[1].request.requestId, "stale")),
    );
    expect(result.current.hits.map((hit) => hit.fileName)).toEqual(["refined"]);
  });

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
      mode: "keyword",
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
        mode: "keyword",
        extensions: ["hwp", "pdf"],
        modifiedAfter: "2026-07-01",
        modifiedBefore: "2026-07-30",
        folderIds: ["documents"],
        includeFilename: false,
        termMode: "all",
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

  it("keeps query syntax intact while sending the selected typed term mode", async () => {
    const search = vi.fn(async (request: SearchRequest) =>
      response(request.requestId, "result.hwp"),
    );
    const cancel = vi.fn().mockResolvedValue(false);
    const { result } = renderHook(() =>
      useImmediateSearch({ search, cancel, debounceMs: 0 }),
    );
    act(() => {
      result.current.patchFilters({ option: "any" });
      result.current.setQuery(
        String.raw`alpha beta path:"C:\My Files" after:2026-01-01 ext:pdf`,
      );
    });
    await waitFor(() => expect(search).toHaveBeenCalled());

    expect(buildBackendQuery(String.raw`"ext:exe" ext:pdf`)).toBe(
      String.raw`"ext:exe" ext:pdf`,
    );
    expect(search).toHaveBeenLastCalledWith(
      expect.objectContaining({
        query: String.raw`alpha beta path:"C:\My Files" after:2026-01-01 ext:pdf`,
        termMode: "any",
      }),
    );
  });

  it("makes a newly selected exact, near, or exclude mode authoritative over typed OR", async () => {
    const search = vi.fn(async (request: SearchRequest) =>
      response(request.requestId, "result.hwp"),
    );
    const cancel = vi.fn().mockResolvedValue(false);
    const { result } = renderHook(() =>
      useImmediateSearch({
        search,
        cancel,
        debounceMs: 0,
      }),
    );

    for (const option of ["exact", "near", "exclude"] as const) {
      act(() => result.current.setQuery("alpha OR beta"));
      await waitFor(() => expect(result.current.filters.option).toBe("any"));
      search.mockClear();

      act(() => result.current.patchFilters({ option }));

      await waitFor(() =>
        expect(search).toHaveBeenLastCalledWith(
          expect.objectContaining({
            query: "alpha beta",
            termMode: option,
          }),
        ),
      );
      expect(result.current.query).toBe("alpha beta");
      expect(result.current.filters.option).toBe(option);
    }
  });

  it("captures decoded quoted extensions in the DTO without rewriting the visible query", async () => {
    const search = vi.fn(async (request: SearchRequest) =>
      response(request.requestId, "result.hwp"),
    );
    const cancel = vi.fn().mockResolvedValue(false);
    const { result } = renderHook(() =>
      useImmediateSearch({
        search,
        cancel,
        debounceMs: 0,
      }),
    );

    act(() => result.current.setQuery(String.raw`alpha ext:"pdf"`));

    await waitFor(() =>
      expect(search).toHaveBeenLastCalledWith(
        expect.objectContaining({
          query: String.raw`alpha ext:"pdf"`,
          extensions: ["pdf"],
        }),
      ),
    );
  });
});
