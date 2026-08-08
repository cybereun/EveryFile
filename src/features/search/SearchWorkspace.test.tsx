import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { FolderRecord, SearchRequest, SearchResponse } from "../../lib/types";
import { SearchWorkspace } from "./SearchWorkspace";

const folders: FolderRecord[] = [
  {
    id: "documents",
    canonicalPath: "C:\\Users\\Lebi\\Documents",
    displayName: "Documents",
    documentCount: 42,
    indexState: "completed",
  },
  {
    id: "downloads",
    canonicalPath: "C:\\Users\\Lebi\\Downloads",
    displayName: "Downloads",
    documentCount: 7,
    indexState: "completed",
  },
];

function searchResponse(request: SearchRequest): SearchResponse {
  return {
    requestId: request.requestId,
    hits: [
      {
        documentId: "filename-hit",
        fileName: "중간고사 계획.hwp",
        path: "C:\\Users\\Lebi\\Documents\\중간고사 계획.hwp",
        extension: "hwp",
        sizeBytes: 1536,
        modifiedAt: "2026-07-29T12:30:00Z",
        snippet: null,
        score: 1,
        matchKind: "filename",
      },
      {
        documentId: "content-hit",
        fileName: "학습 전략.pdf",
        path: "C:\\Users\\Lebi\\Documents\\교육\\학습 전략.pdf",
        extension: "pdf",
        sizeBytes: 2048,
        modifiedAt: "2026-07-28T12:30:00Z",
        snippet:
          '<mark>중간고사</mark> <img src=x onerror="window.__xss=1"> 전략',
        score: 2,
        matchKind: "content",
      },
    ],
    total: 2,
    elapsedMs: 3,
    appliedFilters: [],
    hasMore: false,
  };
}

describe("SearchWorkspace", () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    window.localStorage.clear();
  });

  it("synchronizes extension query syntax, filter chips, dates, and folder scope", async () => {
    const search = vi.fn(async (request: SearchRequest) => searchResponse(request));
    render(
      <SearchWorkspace
        folders={folders}
        searchApi={search}
        cancelApi={vi.fn().mockResolvedValue(false)}
        openApi={vi.fn().mockResolvedValue(undefined)}
        debounceMs={0}
      />,
    );

    const query = screen.getByRole("searchbox", { name: "검색어" });
    fireEvent.change(query, { target: { value: "중간고사 ext:hwp,pdf" } });

    fireEvent.click(screen.getByRole("button", { name: "확장자" }));
    expect(screen.getByRole("checkbox", { name: "HWP" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "PDF" })).toBeChecked();

    fireEvent.click(screen.getByRole("button", { name: "기간" }));
    fireEvent.change(screen.getByLabelText("시작일"), {
      target: { value: "2026-07-01" },
    });
    fireEvent.change(screen.getByLabelText("종료일"), {
      target: { value: "2026-07-30" },
    });
    fireEvent.change(screen.getByLabelText("폴더 범위"), {
      target: { value: "documents" },
    });

    expect(query).toHaveValue("중간고사 ext:hwp,pdf");
    expect(screen.getByRole("button", { name: "확장자 HWP 제거" })).toBeVisible();
    expect(screen.getByRole("button", { name: "기간 필터 제거" })).toBeVisible();
    expect(screen.getByRole("button", { name: "폴더 Documents 제거" })).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "확장자 HWP 제거" }));
    expect(query).toHaveValue("중간고사 ext:pdf");

    await waitFor(() =>
      expect(search).toHaveBeenLastCalledWith(
        expect.objectContaining({
          query: "중간고사 ext:pdf",
          extensions: ["pdf"],
          modifiedAfter: "2026-07-01",
          modifiedBefore: "2026-07-30",
          folderIds: ["documents"],
        }),
      ),
    );
  });

  it("shows the complete detailed controls with narrow overflow", () => {
    render(
      <div style={{ width: 420 }}>
        <SearchWorkspace
          folders={folders}
          searchApi={vi.fn(async (request: SearchRequest) => searchResponse(request))}
          cancelApi={vi.fn().mockResolvedValue(false)}
          openApi={vi.fn().mockResolvedValue(undefined)}
        />
      </div>,
    );

    expect(screen.getByRole("button", { name: "키워드" })).toBeVisible();
    expect(screen.getByRole("button", { name: "파일명" })).toBeVisible();
    expect(screen.getByLabelText("검색 옵션")).toBeVisible();
    expect(screen.getByLabelText("정렬")).toBeVisible();
    expect(screen.getByRole("button", { name: "확장자" })).toBeVisible();
    expect(screen.getByRole("button", { name: "기간" })).toBeVisible();
    expect(screen.getByLabelText("폴더 범위")).toBeVisible();
    expect(screen.getByRole("checkbox", { name: "파일명 포함" })).toBeVisible();
    expect(screen.getByRole("textbox", { name: "결과 내 검색" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "프리셋 저장" })).not.toBeInTheDocument();
    expect(screen.getByTestId("search-filter-scroller")).toHaveStyle({
      overflowX: "auto",
    });
  });

  it("turns a statistics extensionless callback into a real backend search", async () => {
    const search = vi.fn(async (request: SearchRequest) => searchResponse(request));
    render(
      <SearchWorkspace
        folders={folders}
        statisticsFilter={{ extensionless: true }}
        searchApi={search}
        cancelApi={vi.fn().mockResolvedValue(false)}
        openApi={vi.fn().mockResolvedValue(undefined)}
        debounceMs={0}
      />,
    );

    await waitFor(() =>
      expect(search).toHaveBeenCalledWith(
        expect.objectContaining({
          extensionless: true,
          extensions: [],
        }),
      ),
    );
    expect(
      screen.getByRole("button", { name: "확장자 없음 필터 제거" }),
    ).toBeVisible();
  });

  it("groups dense results, supports keyboard open, and never interprets snippet HTML", async () => {
    const open = vi.fn().mockResolvedValue(undefined);
    render(
      <SearchWorkspace
        folders={folders}
        searchApi={vi.fn(async (request: SearchRequest) => searchResponse(request))}
        cancelApi={vi.fn().mockResolvedValue(false)}
        openApi={open}
        debounceMs={0}
      />,
    );

    fireEvent.change(screen.getByRole("searchbox", { name: "검색어" }), {
      target: { value: "중간고사" },
    });
    await screen.findByText("파일명 일치");
    expect(screen.getByText("내용 일치")).toBeVisible();
    expect(screen.getByText("1.5 KB")).toBeVisible();
    expect(screen.getByText("2 KB")).toBeVisible();
    expect(document.querySelector("img")).toBeNull();
    expect(
      screen.getByText(
        (content) => content.includes('<img src=x onerror="window.__xss=1">'),
        { selector: ".result-snippet" },
      ),
    ).toBeVisible();
    expect(screen.getByText("중간고사", { selector: "mark" })).toBeVisible();

    const list = screen.getByRole("listbox", { name: "검색 결과" });
    fireEvent.keyDown(list, { key: "ArrowDown" });
    fireEvent.keyDown(list, { key: "Enter" });
    await waitFor(() => expect(open).toHaveBeenCalledWith("filename-hit"));
  });

  it("filters returned rows with search-within-results", async () => {
    const search = vi.fn(async (request: SearchRequest) => searchResponse(request));
    const cancel = vi.fn().mockResolvedValue(false);
    render(
      <SearchWorkspace
        folders={folders}
        searchApi={search}
        cancelApi={cancel}
        openApi={vi.fn().mockResolvedValue(undefined)}
        debounceMs={0}
      />,
    );

    fireEvent.change(screen.getByRole("searchbox", { name: "검색어" }), {
      target: { value: "중간고사" },
    });
    await screen.findByText("학습 전략.pdf");
    const searchCalls = search.mock.calls.length;
    const cancelCalls = cancel.mock.calls.length;
    fireEvent.change(screen.getByRole("textbox", { name: "결과 내 검색" }), {
      target: { value: "학습" },
    });

    expect(screen.queryByText("중간고사 계획.hwp")).not.toBeInTheDocument();
    expect(screen.getByText("학습 전략.pdf")).toBeVisible();
    await new Promise((resolve) => window.setTimeout(resolve, 150));
    expect(search).toHaveBeenCalledTimes(searchCalls);
    expect(cancel).toHaveBeenCalledTimes(cancelCalls);
  });

  it("renders popovers in a portal outside the overflow scroller", () => {
    render(
      <div style={{ width: 420 }}>
        <SearchWorkspace
          folders={folders}
          searchApi={vi.fn(async (request: SearchRequest) => searchResponse(request))}
          cancelApi={vi.fn().mockResolvedValue(false)}
          openApi={vi.fn().mockResolvedValue(undefined)}
        />
      </div>,
    );
    const extensionButton = screen.getByRole("button", { name: "확장자" });
    vi.spyOn(extensionButton, "getBoundingClientRect").mockReturnValue({
      x: 370,
      y: 40,
      top: 40,
      right: 430,
      bottom: 72,
      left: 370,
      width: 60,
      height: 32,
      toJSON: () => ({}),
    });
    Object.defineProperty(window, "innerWidth", {
      configurable: true,
      value: 420,
    });

    fireEvent.click(extensionButton);
    const menu = screen.getByRole("dialog", { name: "확장자 필터" });
    expect(screen.getByTestId("search-filter-scroller")).not.toContainElement(menu);
    expect(menu.parentElement).toBe(document.body);
    expect(Number.parseFloat(menu.style.left)).toBeLessThanOrEqual(236);

    fireEvent.keyDown(document, { key: "Escape" });
    expect(
      screen.queryByRole("dialog", { name: "확장자 필터" }),
    ).not.toBeInTheDocument();
  });

  it("explains and disables unsupported filename-only combinations", () => {
    render(
      <SearchWorkspace
        folders={folders}
        searchApi={vi.fn(async (request: SearchRequest) => searchResponse(request))}
        cancelApi={vi.fn().mockResolvedValue(false)}
        openApi={vi.fn().mockResolvedValue(undefined)}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "파일명" }));

    expect(screen.getByRole("option", { name: /인접 검색/ })).toBeDisabled();
    expect(screen.getByRole("option", { name: /신뢰도순/ })).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: "파일명 포함" })).toBeDisabled();
    expect(screen.getByText(/파일명 검색에서는/)).toBeVisible();
  });

  it("saves the current query and detailed filters for immediate reuse", () => {
    render(
      <SearchWorkspace
        folders={folders}
        searchApi={vi.fn(async (request: SearchRequest) => searchResponse(request))}
        cancelApi={vi.fn().mockResolvedValue(false)}
        openApi={vi.fn().mockResolvedValue(undefined)}
      />,
    );
    fireEvent.change(screen.getByRole("searchbox", { name: "검색어" }), {
      target: { value: "수행평가 ext:pdf" },
    });
    expect(window.localStorage.getItem("everyfile.search.current")).toContain(
      "수행평가 ext:pdf",
    );
  });
});
