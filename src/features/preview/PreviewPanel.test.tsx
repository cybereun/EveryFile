import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PreviewBlock, PreviewDocument } from "../../lib/types";
import { DocumentTextView } from "./DocumentTextView";
import { PdfLayoutView, type PdfDocumentLike } from "./PdfLayoutView";
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
};

describe("secure document preview", () => {
  afterEach(cleanup);

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
    expect(screen.getByRole("button", { name: "파일 열기" })).toBeVisible();
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

  it("explains that non-PDF original layout is unavailable without conversion", async () => {
    render(
      <PreviewPanel
        documentId="doc-1"
        getPreviewApi={vi.fn().mockResolvedValue({
          ...preview,
          fileName: "report.hwp",
          extension: "hwp",
        })}
      />,
    );
    await screen.findByText("report.hwp");
    fireEvent.click(screen.getByRole("tab", { name: "원본 레이아웃" }));
    expect(
      screen.getByText("이 형식은 문서 텍스트로만 미리볼 수 있습니다."),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "파일 열기" })).toBeVisible();
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
        loader={vi.fn().mockResolvedValue(pdfDocument)}
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
});
