import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PreviewBlock, PreviewDocument } from "../../lib/types";
import { DocumentTextView } from "./DocumentTextView";
import {
  PdfLayoutView,
  type PdfDocumentLike,
  type PdfLoadingTaskLike,
} from "./PdfLayoutView";
import { PreviewPanel } from "./PreviewPanel";

function paragraphs(text: string): PreviewBlock[] {
  return [{ type: "paragraph", text, level: null, pageNumber: null }];
}

const preview: PreviewDocument = {
  documentId: "doc-1",
  fileName: "중간고사.pdf",
  path: "C:\\Documents\\중간고사.pdf",
  extension: "pdf",
  markdown: "# 중간고사",
  blocks: paragraphs("중간고사 준비 중간고사"),
  warnings: [],
  bookmarked: false,
  bookmarkNote: "",
  tags: [],
  truncated: false,
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((onResolve, onReject) => {
    resolve = onResolve;
    reject = onReject;
  });
  return { promise, reject, resolve };
}

function loadingTask(document: PdfDocumentLike): PdfLoadingTaskLike {
  return { promise: Promise.resolve(document), destroy: vi.fn() };
}

describe("secure document preview", () => {
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("finds and moves between matches in document text", () => {
    render(<DocumentTextView blocks={paragraphs("중간고사 준비 중간고사")} />);

    fireEvent.keyDown(window, { key: "f", ctrlKey: true });
    fireEvent.change(
      screen.getByRole("searchbox", { name: "문서 내 찾기" }),
      { target: { value: "중간고사" } },
    );
    expect(screen.getByText("1 / 2")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "다음 일치" }));
    expect(screen.getByText("2 / 2")).toBeVisible();
  });

  it("renders structured source as text, preserves tables, and allowlists links", () => {
    const blocks: PreviewBlock[] = [
      {
        type: "heading",
        text: "<img src=x onerror=alert(1)>",
        level: 2,
        pageNumber: null,
        href: "javascript:alert(1)",
      },
      {
        type: "paragraph",
        text: "공식 문서",
        level: null,
        pageNumber: null,
        href: "https://example.com",
      },
      {
        type: "table",
        text: "",
        level: null,
        pageNumber: null,
        table: {
          rows: 1,
          cols: 1,
          hasHeader: true,
          cells: [[{ text: "<script>x</script>", colSpan: 1, rowSpan: 1 }]],
        },
      },
    ];
    render(<DocumentTextView blocks={blocks} />);

    expect(document.querySelector("img")).toBeNull();
    expect(document.querySelector("script")).toBeNull();
    expect(screen.getByRole("heading")).toHaveTextContent("<img");
    expect(screen.getByRole("link", { name: "공식 문서" })).toHaveAttribute(
      "href",
      "https://example.com",
    );
    expect(screen.getByRole("table")).toHaveTextContent("<script>x</script>");
  });

  it("loads the selected document, exposes approved actions, and keeps AI absent", async () => {
    const open = vi.fn().mockResolvedValue(undefined);
    render(
      <PreviewPanel
        documentId="doc-1"
        getPreviewApi={vi.fn().mockResolvedValue(preview)}
        openApi={open}
        pdfBytesApi={vi.fn().mockResolvedValue([])}
      />,
    );

    await screen.findByText("중간고사.pdf");
    expect(screen.getAllByRole("button", { name: "파일 열기" })[0]).toBeVisible();
    expect(screen.getByRole("button", { name: "찾기" })).toBeVisible();
    expect(screen.getByRole("button", { name: "북마크 추가" })).toBeVisible();
    expect(screen.getByRole("button", { name: "더보기" })).toBeVisible();
    expect(screen.queryByText(/AI/i)).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "파일 열기" }));
    await waitFor(() => expect(open).toHaveBeenCalledWith("doc-1"));

    fireEvent.click(screen.getByRole("button", { name: "더보기" }));
    expect(screen.getByRole("menuitem", { name: "파일 위치 열기" })).toBeVisible();
    expect(screen.getByRole("menuitem", { name: "텍스트 복사" })).toBeVisible();
    expect(screen.getByRole("menuitem", { name: "Markdown 저장" })).toBeVisible();
    expect(screen.getByRole("menuitem", { name: "경로 복사" })).toBeVisible();
    expect(screen.getByRole("menuitem", { name: "태그 추가" })).toBeVisible();
  });

  it("supports keyboard navigation and restores focus when the more menu closes", async () => {
    render(
      <PreviewPanel
        documentId="doc-1"
        getPreviewApi={vi.fn().mockResolvedValue(preview)}
      />,
    );

    await screen.findByText(preview.fileName);
    const trigger = screen.getByRole("button", { name: "더보기" });
    fireEvent.click(trigger);
    const first = screen.getByRole("menuitem", { name: "파일 위치 열기" });
    await waitFor(() => expect(first).toHaveFocus());

    fireEvent.keyDown(document, { key: "End" });
    expect(screen.getByRole("menuitem", { name: "태그 추가" })).toHaveFocus();
    fireEvent.keyDown(document, { key: "ArrowDown" });
    expect(first).toHaveFocus();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });

  it("shows AI actions only when enabled and runs an Ollama summary", async () => {
    const runAi = vi.fn().mockResolvedValue("핵심 요약입니다.");
    render(
      <PreviewPanel
        documentId="doc-1"
        getPreviewApi={vi.fn().mockResolvedValue(preview)}
        aiEnabled
        aiProvider="ollama"
        runAiApi={runAi}
      />,
    );

    await screen.findByText(preview.fileName);
    fireEvent.click(screen.getByRole("button", { name: "AI 요약" }));
    expect(screen.getByRole("region", { name: "문서 AI" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "실행" }));

    await screen.findByText("핵심 요약입니다.");
    expect(runAi).toHaveBeenCalledWith(
      expect.any(String),
      "doc-1",
      null,
      true,
    );
  });

  it("requires remote consent and cancels an active document AI request", async () => {
    const runAi = vi.fn(
      (
        _requestId: string,
        _documentId: string,
        _question: string | null,
        _remoteConsent: boolean,
      ) => new Promise<string>(() => undefined),
    );
    const cancelAi = vi.fn().mockResolvedValue(true);
    render(
      <PreviewPanel
        cancelAiApi={cancelAi}
        documentId="doc-1"
        getPreviewApi={vi.fn().mockResolvedValue(preview)}
        aiEnabled
        aiProvider="openai"
        runAiApi={runAi}
      />,
    );

    await screen.findByText(preview.fileName);
    fireEvent.click(screen.getByRole("button", { name: "AI 요약" }));
    const execute = screen.getByRole("button", { name: "실행" });
    expect(execute).toBeDisabled();
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(execute);
    await waitFor(() => expect(runAi).toHaveBeenCalled());
    fireEvent.click(screen.getByRole("button", { name: "취소" }));
    await waitFor(() =>
      expect(cancelAi).toHaveBeenCalledWith(runAi.mock.calls[0][0]),
    );
  });

  it("renders HWP original layout with the local document renderer", async () => {
    const free = vi.fn();
    const hwpLoader = vi.fn().mockResolvedValue({
      pageCount: () => 1,
      renderPageSvg: () =>
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 600 800"><text x="20" y="40">시험 문서</text></svg>',
      free,
    });
    render(
      <PreviewPanel
        documentId="doc-1"
        getPreviewApi={vi.fn().mockResolvedValue({
          ...preview,
          fileName: "report.hwp",
          extension: "hwp",
        })}
        hwpBytesApi={vi.fn().mockResolvedValue([1, 2, 3])}
        hwpLoader={hwpLoader}
      />,
    );
    await screen.findByText("report.hwp");
    fireEvent.click(screen.getByRole("tab", { name: "원본 레이아웃" }));
    await screen.findByLabelText("HWP 1페이지");
    expect(hwpLoader).toHaveBeenCalledWith(new Uint8Array([1, 2, 3]));
  });

  it("renders one PDF canvas while prefetching only the adjacent page window", async () => {
    const context = {
      setTransform: vi.fn(),
    } as unknown as CanvasRenderingContext2D;
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(context);
    const getPage = vi.fn(async () => ({
      getViewport: ({ scale }: { scale: number }) => ({
        width: 600 * scale,
        height: 800 * scale,
      }),
      render: () => ({ promise: Promise.resolve(), cancel: vi.fn() }),
      cleanup: vi.fn(),
    }));
    const pdfDocument: PdfDocumentLike = {
      numPages: 5,
      getPage,
      destroy: vi.fn(),
    };
    render(
      <PdfLayoutView
        documentId="doc-1"
        getBytesApi={vi.fn().mockResolvedValue(
          new Uint8Array([37, 80, 68, 70, 45]).buffer,
        )}
        loader={vi.fn().mockReturnValue(loadingTask(pdfDocument))}
      />,
    );

    await screen.findByText("1 / 5");
    await waitFor(() => expect(getPage).toHaveBeenCalledTimes(2));
    expect(getPage).toHaveBeenCalledWith(1);
    expect(getPage).toHaveBeenCalledWith(2);
    expect(document.querySelectorAll("canvas")).toHaveLength(1);
    expect(screen.getByRole("button", { name: "확대" })).toBeVisible();
    expect(screen.getByRole("button", { name: "축소" })).toBeVisible();
    expect(screen.getByRole("button", { name: "전체 화면" })).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "다음 페이지" }));
    await screen.findByText("2 / 5");
    await waitFor(() => expect(getPage).toHaveBeenCalledWith(3));
    expect(getPage).not.toHaveBeenCalledWith(4);
  });

  it("recomputes fit width after bounded container resizes", async () => {
    const observers: ResizeObserverCallback[] = [];
    vi.stubGlobal(
      "ResizeObserver",
      class {
        constructor(callback: ResizeObserverCallback) {
          observers.push(callback);
        }
        observe() {}
        disconnect() {}
        unobserve() {}
      },
    );
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
      setTransform: vi.fn(),
    } as unknown as CanvasRenderingContext2D);
    const page = {
      getViewport: ({ scale }: { scale: number }) => ({
        width: 600 * scale,
        height: 800 * scale,
      }),
      render: () => ({ promise: Promise.resolve(), cancel: vi.fn() }),
      cleanup: vi.fn(),
    };
    render(
      <PdfLayoutView
        documentId="doc-1"
        getBytesApi={vi.fn().mockResolvedValue(new ArrayBuffer(8))}
        loader={vi.fn().mockReturnValue(
          loadingTask({ numPages: 1, getPage: vi.fn().mockResolvedValue(page) }),
        )}
      />,
    );
    const canvas = await screen.findByLabelText("PDF 1페이지");

    observers[0](
      [{ contentRect: { width: 400 } } as ResizeObserverEntry],
      {} as ResizeObserver,
    );
    await waitFor(() => expect(canvas).toHaveStyle({ width: "376px" }));

    observers[0](
      [{ contentRect: { width: 800 } } as ResizeObserverEntry],
      {} as ResizeObserver,
    );
    await waitFor(() => expect(canvas).toHaveStyle({ width: "775px" }));
  });

  it("cancels superseded PDF reads and never parses their late bytes", async () => {
    const first = deferred<ArrayBuffer>();
    const second = deferred<ArrayBuffer>();
    const getBytes = vi
      .fn()
      .mockImplementationOnce(() => first.promise)
      .mockImplementationOnce(() => second.promise);
    const cancel = vi.fn().mockResolvedValue(true);
    const pdfDocument: PdfDocumentLike = {
      numPages: 1,
      getPage: vi.fn(),
    };
    const loader = vi.fn().mockReturnValue(loadingTask(pdfDocument));
    const { rerender } = render(
      <PdfLayoutView
        cancelReadApi={cancel}
        documentId="doc-a"
        getBytesApi={getBytes}
        loader={loader}
      />,
    );
    await waitFor(() => expect(getBytes).toHaveBeenCalledTimes(1));
    const firstRequestId = getBytes.mock.calls[0][1];

    rerender(
      <PdfLayoutView
        cancelReadApi={cancel}
        documentId="doc-b"
        getBytesApi={getBytes}
        loader={loader}
      />,
    );
    await waitFor(() => expect(cancel).toHaveBeenCalledWith(firstRequestId));
    first.resolve(new Uint8Array([1]).buffer);
    second.resolve(new Uint8Array([2]).buffer);
    await waitFor(() => expect(loader).toHaveBeenCalledTimes(1));
    expect(new Uint8Array(loader.mock.calls[0][0])).toEqual(new Uint8Array([2]));
  });

  it.each([
    ["PasswordException", "암호화된 PDF"],
    ["InvalidPDFException", "손상되었거나 올바르지 않은 PDF"],
  ])("shows a safe error for %s", async (name, message) => {
    const failure = Object.assign(new Error("parser detail"), { name });
    render(
      <PdfLayoutView
        documentId="doc-1"
        getBytesApi={vi.fn().mockResolvedValue(new ArrayBuffer(8))}
        loader={vi.fn().mockReturnValue({
          promise: Promise.reject(failure),
          destroy: vi.fn(),
        })}
      />,
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(message);
  });

  it("decodes a structured Tauri PDF size error without exposing backend text", async () => {
    render(
      <PdfLayoutView
        documentId="doc-1"
        getBytesApi={vi.fn().mockRejectedValue({
          code: "SOURCE_PDF_TOO_LARGE",
          message: "indexed PDF exceeds the preview size limit",
        })}
        loader={vi.fn()}
      />,
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "PDF 파일이 미리보기 크기 제한을 초과했습니다.",
    );
  });

  it("rejects a page that exceeds the canvas pixel budget", async () => {
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
      setTransform: vi.fn(),
    } as unknown as CanvasRenderingContext2D);
    const page = {
      getViewport: ({ scale }: { scale: number }) => ({
        width: 100_000 * scale,
        height: 100_000 * scale,
      }),
      render: vi.fn(),
      cleanup: vi.fn(),
    };
    render(
      <PdfLayoutView
        documentId="doc-1"
        getBytesApi={vi.fn().mockResolvedValue(new ArrayBuffer(8))}
        loader={vi.fn().mockReturnValue(
          loadingTask({ numPages: 1, getPage: vi.fn().mockResolvedValue(page) }),
        )}
      />,
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "PDF 페이지가 안전한 표시 한도를 초과했습니다.",
    );
    expect(page.render).not.toHaveBeenCalled();
  });

  it("does not apply late bookmark or tag completions to a newly selected document", async () => {
    const bookmark = deferred<{
      documentId: string;
      note: string;
      createdAt: string;
    }>();
    const saveTags = deferred<PreviewDocument["tags"]>();
    const getPreviewApi = vi.fn(async (documentId: string) => ({
      ...preview,
      documentId,
      fileName: `${documentId}.pdf`,
    }));
    const { rerender } = render(
      <PreviewPanel
        createTagApi={vi.fn().mockResolvedValue({
          id: "tag-a",
          name: "A",
          color: "terracotta",
        })}
        documentId="doc-a"
        getPreviewApi={getPreviewApi}
        setBookmarkApi={vi.fn(() => bookmark.promise)}
        setTagsApi={vi.fn(() => saveTags.promise)}
      />,
    );
    await screen.findByText("doc-a.pdf");
    fireEvent.click(screen.getByRole("button", { name: "북마크 추가" }));
    fireEvent.click(screen.getByRole("button", { name: "더보기" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "태그 추가" }));
    fireEvent.change(screen.getByRole("textbox", { name: "새 태그 이름" }), {
      target: { value: "A" },
    });
    fireEvent.click(screen.getByRole("button", { name: "태그 만들기" }));

    rerender(
      <PreviewPanel
        createTagApi={vi.fn().mockResolvedValue({
          id: "tag-a",
          name: "A",
          color: "terracotta",
        })}
        documentId="doc-b"
        getPreviewApi={getPreviewApi}
        setBookmarkApi={vi.fn(() => bookmark.promise)}
        setTagsApi={vi.fn(() => saveTags.promise)}
      />,
    );
    await screen.findByText("doc-b.pdf");
    expect(screen.getByRole("dialog", { name: "태그 편집" })).toBeVisible();
    await act(async () => {
      bookmark.resolve({ documentId: "doc-a", note: "", createdAt: "" });
      saveTags.resolve([{ id: "tag-a", name: "A", color: "terracotta" }]);
      await saveTags.promise;
    });
    expect(screen.getByRole("button", { name: "북마크 추가" })).toBeVisible();
    expect(screen.queryByText("A", { selector: ".preview-tag" })).not.toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "태그 편집" })).toBeVisible();
  });
});
