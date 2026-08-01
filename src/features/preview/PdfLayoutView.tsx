import { useEffect, useRef, useState, type ReactNode } from "react";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import { cancelPdfRead, getPdfBytes } from "../../lib/ipc";

const RESIZE_DEBOUNCE_MS = 80;
const MAX_CANVAS_DIMENSION = 8192;
const MAX_CANVAS_PIXELS = 16_000_000;
const MAX_IMAGE_PIXELS = 16_000_000;
const MAX_CANVAS_BYTES = 64 * 1024 * 1024;
let requestSequence = 0;

interface PdfPageLike {
  getViewport(options: { scale: number }): { width: number; height: number };
  getTextContent?(): Promise<{
    items: Array<{ str?: string; transform?: number[]; width?: number; height?: number }>;
  }>;
  render(options: {
    canvas: HTMLCanvasElement;
    canvasContext: CanvasRenderingContext2D;
    viewport: { width: number; height: number };
  }): { promise: Promise<unknown>; cancel?: () => void };
  cleanup?: () => void;
}

export interface PdfDocumentLike {
  numPages: number;
  getPage(pageNumber: number): Promise<PdfPageLike>;
  destroy?: () => Promise<void> | void;
}

export interface PdfLoadingTaskLike {
  promise: Promise<PdfDocumentLike>;
  destroy?: () => Promise<void> | void;
}

export type PdfLoader = (
  bytes: Uint8Array,
) => PdfLoadingTaskLike | Promise<PdfLoadingTaskLike>;

async function loadLocalPdf(bytes: Uint8Array): Promise<PdfLoadingTaskLike> {
  const pdfjs = await import("pdfjs-dist");
  pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;
  return pdfjs.getDocument({
    data: bytes,
    disableAutoFetch: true,
    disableStream: true,
    maxImageSize: MAX_IMAGE_PIXELS,
    canvasMaxAreaInBytes: MAX_CANVAS_BYTES,
  }) as unknown as PdfLoadingTaskLike;
}

function nextRequestId() {
  requestSequence += 1;
  return `pdf-${Date.now().toString(36)}-${requestSequence.toString(36)}`;
}

function safePdfError(caught: unknown) {
  const code =
    caught && typeof caught === "object" && "code" in caught
      ? String(caught.code)
      : "";
  if (code === "SOURCE_PDF_TOO_LARGE") {
    return "PDF 파일이 미리보기 크기 제한을 초과했습니다.";
  }
  if (code === "PDF_PASSWORD_REQUIRED" || code === "PDF_ENCRYPTED") {
    return "암호화된 PDF는 원본 레이아웃으로 미리볼 수 없습니다.";
  }
  if (code === "SOURCE_NOT_PDF" || code === "PDF_INVALID" || code === "PDF_MALFORMED") {
    return "손상되었거나 올바르지 않은 PDF입니다.";
  }
  if (code === "SOURCE_NOT_FOUND" || code === "SOURCE_UNAVAILABLE") {
    return "PDF 파일을 읽을 수 없습니다.";
  }
  if (code === "PDF_READ_CANCELLED" || code === "SOURCE_PDF_READ_CANCELLED") {
    return null;
  }
  const name =
    caught && typeof caught === "object" && "name" in caught
      ? String(caught.name)
      : "";
  if (name === "PasswordException") {
    return "암호화된 PDF는 원본 레이아웃으로 미리볼 수 없습니다.";
  }
  if (name === "InvalidPDFException" || name === "FormatError") {
    return "손상되었거나 올바르지 않은 PDF입니다.";
  }
  if (name === "MissingPDFException") {
    return "PDF 파일을 읽을 수 없습니다.";
  }
  if (
    name === "AbortException" ||
    (caught instanceof Error &&
      /PDF_READ_CANCELLED|SOURCE_PDF_READ_CANCELLED/.test(caught.message))
  ) {
    return null;
  }
  return caught instanceof Error && caught.message
    ? caught.message
    : "PDF를 불러오지 못했습니다.";
}

interface PdfLayoutViewProps {
  documentId: string;
  initialQuery?: string;
  findRequest?: number;
  getBytesApi?: typeof getPdfBytes;
  cancelReadApi?: typeof cancelPdfRead;
  loader?: PdfLoader;
}

export function PdfLayoutView({
  documentId,
  initialQuery = "",
  findRequest = 0,
  getBytesApi = getPdfBytes,
  cancelReadApi = cancelPdfRead,
  loader = loadLocalPdf,
}: PdfLayoutViewProps) {
  const container = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const pages = useRef(new Map<number, PdfPageLike>());
  const [document, setDocument] = useState<PdfDocumentLike | null>(null);
  const [pageNumber, setPageNumber] = useState(1);
  const [zoom, setZoom] = useState(1);
  const [fitWidth, setFitWidth] = useState(true);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [fitRevision, setFitRevision] = useState(0);
  const [containerWidth, setContainerWidth] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [findOpen, setFindOpen] = useState(Boolean(initialQuery.trim()));
  const [query, setQuery] = useState(initialQuery);
  const [activeMatch, setActiveMatch] = useState(0);
  const [textItems, setTextItems] = useState<Array<{
    text: string;
    left: number;
    top: number;
    width: number;
    fontSize: number;
  }>>([]);

  useEffect(() => {
    if (findRequest > 0) setFindOpen(true);
  }, [findRequest]);

  useEffect(() => {
    setQuery(initialQuery);
    setFindOpen(Boolean(initialQuery.trim()));
    setActiveMatch(0);
  }, [documentId, initialQuery]);

  useEffect(() => {
    const requestId = nextRequestId();
    let active = true;
    let loadingTask: PdfLoadingTaskLike | null = null;
    setLoading(true);
    setError(null);
    setDocument(null);
    setPageNumber(1);
    pages.current.forEach((page) => page.cleanup?.());
    pages.current.clear();

    void getBytesApi(documentId, requestId)
      .then(async (bytes) => {
        if (!active) return null;
        const task = await loader(new Uint8Array(bytes));
        if (!active) {
          await task.destroy?.();
          return null;
        }
        loadingTask = task;
        return task.promise;
      })
      .then((nextDocument) => {
        if (active && nextDocument) setDocument(nextDocument);
      })
      .catch((caught) => {
        if (!active) return;
        const message = safePdfError(caught);
        if (message) setError(message);
      })
      .finally(() => {
        if (active) setLoading(false);
      });

    return () => {
      active = false;
      void cancelReadApi(requestId).catch(() => undefined);
      pages.current.forEach((page) => page.cleanup?.());
      pages.current.clear();
      void loadingTask?.destroy?.();
    };
  }, [cancelReadApi, documentId, getBytesApi, loader]);

  useEffect(() => {
    if (!document || !container.current || typeof ResizeObserver !== "function") return;
    let timer: number | undefined;
    const update = (width: number) => {
      const bounded = Math.max(200, Math.min(MAX_CANVAS_DIMENSION, Math.floor(width)));
      window.clearTimeout(timer);
      timer = window.setTimeout(() => setContainerWidth(bounded), RESIZE_DEBOUNCE_MS);
    };
    update(container.current.clientWidth);
    const observer = new ResizeObserver((entries) => {
      const width = entries[0]?.contentRect.width;
      if (Number.isFinite(width)) update(width);
    });
    observer.observe(container.current);
    return () => {
      window.clearTimeout(timer);
      observer.disconnect();
    };
  }, [document]);

  useEffect(() => {
    if (!document) return;
    let active = true;
    let renderTask: ReturnType<PdfPageLike["render"]> | null = null;
    const keep = new Set(
      [pageNumber - 1, pageNumber, pageNumber + 1].filter(
        (page) => page >= 1 && page <= document.numPages,
      ),
    );
    for (const [number, page] of pages.current) {
      if (!keep.has(number)) {
        page.cleanup?.();
        pages.current.delete(number);
      }
    }

    const getPage = async (number: number) => {
      const cached = pages.current.get(number);
      if (cached) return cached;
      const page = await document.getPage(number);
      if (!page || typeof page.getViewport !== "function" || typeof page.render !== "function") {
        throw Object.assign(new Error("invalid PDF page"), { name: "FormatError" });
      }
      if (active && keep.has(number)) pages.current.set(number, page);
      else page.cleanup?.();
      return page;
    };

    void Promise.all([...keep].map(getPage))
      .then(async () => {
        if (!active) return;
        const page = pages.current.get(pageNumber);
        const target = canvas.current;
        const context = target?.getContext("2d");
        if (!page || !target || !context) return;
        const base = page.getViewport({ scale: 1 });
        if (
          !Number.isFinite(base.width) ||
          !Number.isFinite(base.height) ||
          base.width <= 0 ||
          base.height <= 0
        ) {
          throw Object.assign(new Error("invalid PDF page dimensions"), {
            name: "FormatError",
          });
        }
        const availableWidth = Math.max(
          200,
          (containerWidth || container.current?.clientWidth || base.width) - 24,
        );
        const scale = fitWidth
          ? Math.min(4, Math.max(0.25, availableWidth / base.width))
          : zoom;
        const viewport = page.getViewport({ scale });
        const ratio = Math.min(2, window.devicePixelRatio || 1);
        const pixelWidth = Math.floor(viewport.width * ratio);
        const pixelHeight = Math.floor(viewport.height * ratio);
        if (
          !Number.isFinite(pixelWidth) ||
          !Number.isFinite(pixelHeight) ||
          pixelWidth <= 0 ||
          pixelHeight <= 0 ||
          pixelWidth > MAX_CANVAS_DIMENSION ||
          pixelHeight > MAX_CANVAS_DIMENSION ||
          pixelWidth * pixelHeight > MAX_CANVAS_PIXELS
        ) {
          throw new Error("PDF_PAGE_RENDER_LIMIT");
        }
        setError(null);
        target.width = pixelWidth;
        target.height = pixelHeight;
        target.style.width = `${Math.floor(viewport.width)}px`;
        target.style.height = `${Math.floor(viewport.height)}px`;
        context.setTransform(ratio, 0, 0, ratio, 0, 0);
        renderTask = page.render({ canvas: target, canvasContext: context, viewport });
        await renderTask.promise;
        if (page.getTextContent) {
          const content = await page.getTextContent();
          if (!active) return;
          setTextItems(
            content.items.flatMap((item) => {
              const transform = item.transform;
              const text = item.str ?? "";
              if (!transform || transform.length < 6 || !text) return [];
              const fontSize = Math.max(6, Math.hypot(transform[2] ?? 0, transform[3] ?? 0) * scale);
              return [{
                text,
                left: (transform[4] ?? 0) * scale,
                top: viewport.height - (transform[5] ?? 0) * scale - fontSize,
                width: Math.max(1, (item.width ?? 0) * scale),
                fontSize,
              }];
            }),
          );
        } else {
          setTextItems([]);
        }
      })
      .catch((caught) => {
        if (!active || (caught as { name?: string }).name === "RenderingCancelledException") {
          return;
        }
        setError(
          caught instanceof Error && caught.message === "PDF_PAGE_RENDER_LIMIT"
            ? "PDF 페이지가 안전한 표시 한도를 초과했습니다."
            : safePdfError(caught) ?? "PDF 페이지 표시가 취소되었습니다.",
        );
      });

    return () => {
      active = false;
      renderTask?.cancel?.();
    };
  }, [containerWidth, document, fitRevision, fitWidth, pageNumber, zoom]);

  useEffect(() => {
    const handleFullscreenChange = () => {
      setIsFullscreen(globalThis.document.fullscreenElement === container.current);
    };
    globalThis.document.addEventListener("fullscreenchange", handleFullscreenChange);
    handleFullscreenChange();
    return () => globalThis.document.removeEventListener("fullscreenchange", handleFullscreenChange);
  }, []);

  const toggleFullscreen = async () => {
    const element = container.current;
    if (!element) return;
    try {
      if (globalThis.document.fullscreenElement === element) {
        await globalThis.document.exitFullscreen();
      } else {
        await element.requestFullscreen();
      }
    } catch {
      setIsFullscreen(false);
    }
  };

  if (loading) return <div className="preview-message">PDF 불러오는 중…</div>;
  if (error) return <div className="preview-message preview-message--error" role="alert">{error}</div>;
  if (!document) return null;

  const normalizedQuery = query.trim().toLocaleLowerCase();
  const totalMatches = textItems.reduce((total, item) => {
    if (!normalizedQuery) return total;
    const source = item.text.toLocaleLowerCase();
    let offset = 0;
    let count = 0;
    while ((offset = source.indexOf(normalizedQuery, offset)) !== -1) {
      count += 1;
      offset += Math.max(1, normalizedQuery.length);
    }
    return total + count;
  }, 0);
  let matchIndex = 0;
  const highlightText = (text: string) => {
    if (!normalizedQuery) return text;
    const source = text.toLocaleLowerCase();
    const output: ReactNode[] = [];
    let cursor = 0;
    let found = source.indexOf(normalizedQuery);
    while (found !== -1) {
      if (found > cursor) output.push(text.slice(cursor, found));
      const current = matchIndex++;
      output.push(<mark className={current === activeMatch ? "is-active" : undefined} key={`${found}-${current}`}>{text.slice(found, found + query.trim().length)}</mark>);
      cursor = found + query.trim().length;
      found = source.indexOf(normalizedQuery, cursor);
    }
    output.push(text.slice(cursor));
    return output;
  };
  const moveMatch = (direction: number) => {
    if (!totalMatches) return;
    setActiveMatch((current) => (current + direction + totalMatches) % totalMatches);
  };

  return (
    <section className="pdf-layout-view" ref={container}>
      <div className="pdf-controls" aria-label="PDF 보기 도구">
        <button aria-label="이전 페이지" disabled={pageNumber <= 1} onClick={() => setPageNumber((page) => Math.max(1, page - 1))} type="button">‹</button>
        <span>{pageNumber} / {document.numPages}</span>
        <button aria-label="다음 페이지" disabled={pageNumber >= document.numPages} onClick={() => setPageNumber((page) => Math.min(document.numPages, page + 1))} type="button">›</button>
        <button aria-label="축소" onClick={() => { setFitWidth(false); setZoom((value) => Math.max(0.25, value - 0.25)); }} type="button">−</button>
        <button aria-label="확대" onClick={() => { setFitWidth(false); setZoom((value) => Math.min(4, value + 0.25)); }} type="button">+</button>
        <button
          aria-pressed={fitWidth}
          onClick={() => {
            setFitWidth(true);
            setFitRevision((revision) => revision + 1);
          }}
          type="button"
        >
          너비 맞춤
        </button>
        <button
          aria-label={isFullscreen ? "원래 크기로" : "전체 화면"}
          aria-pressed={isFullscreen}
          onClick={() => void toggleFullscreen()}
          type="button"
        >
          {isFullscreen ? "⤢" : "⛶"}
        </button>
      </div>
      {findOpen && (
        <div className="document-find pdf-layout-find">
          <input aria-label="원본에서 찾기" onChange={(event) => { setQuery(event.target.value); setActiveMatch(0); }} placeholder="원본에서 찾기" type="search" value={query} />
          <span aria-live="polite">{totalMatches ? `${activeMatch + 1} / ${totalMatches}` : "0 / 0"}</span>
          <button aria-label="이전 일치" onClick={() => moveMatch(-1)} type="button">↑</button>
          <button aria-label="다음 일치" onClick={() => moveMatch(1)} type="button">↓</button>
          <button aria-label="찾기 닫기" onClick={() => setFindOpen(false)} type="button">×</button>
        </div>
      )}
      <div className="pdf-canvas-wrap">
        <div className="pdf-page-surface">
          <canvas aria-label={`PDF ${pageNumber}페이지`} ref={canvas} />
          <div className="pdf-text-layer" aria-hidden="true">
            {textItems.map((item, index) => (
              <span key={index} style={{ left: item.left, top: item.top, width: item.width, fontSize: item.fontSize }}>{highlightText(item.text)}</span>
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}
