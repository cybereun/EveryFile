import { useEffect, useRef, useState } from "react";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import { getPdfBytes } from "../../lib/ipc";

interface PdfPageLike {
  getViewport(options: { scale: number }): { width: number; height: number };
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

export type PdfLoader = (bytes: Uint8Array) => Promise<PdfDocumentLike>;

async function loadLocalPdf(bytes: Uint8Array): Promise<PdfDocumentLike> {
  const pdfjs = await import("pdfjs-dist");
  pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;
  return pdfjs.getDocument({
    data: bytes,
    disableAutoFetch: true,
    disableStream: true,
  }).promise as unknown as Promise<PdfDocumentLike>;
}

interface PdfLayoutViewProps {
  documentId: string;
  getBytesApi?: typeof getPdfBytes;
  loader?: PdfLoader;
}

export function PdfLayoutView({
  documentId,
  getBytesApi = getPdfBytes,
  loader = loadLocalPdf,
}: PdfLayoutViewProps) {
  const container = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const pages = useRef(new Map<number, PdfPageLike>());
  const [document, setDocument] = useState<PdfDocumentLike | null>(null);
  const [pageNumber, setPageNumber] = useState(1);
  const [zoom, setZoom] = useState(1);
  const [fitWidth, setFitWidth] = useState(true);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    let loaded: PdfDocumentLike | null = null;
    setLoading(true);
    setError(null);
    setPageNumber(1);
    pages.current.clear();
    void getBytesApi(documentId)
      .then((bytes) => loader(new Uint8Array(bytes)))
      .then((nextDocument) => {
        loaded = nextDocument;
        if (active) setDocument(nextDocument);
        else void nextDocument.destroy?.();
      })
      .catch((caught) => {
        if (active) {
          setError(caught instanceof Error ? caught.message : "PDF를 불러오지 못했습니다.");
        }
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
      pages.current.forEach((page) => page.cleanup?.());
      pages.current.clear();
      setDocument(null);
      if (loaded) void loaded.destroy?.();
    };
  }, [documentId, getBytesApi, loader]);

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
      if (active && keep.has(number)) pages.current.set(number, page);
      else page.cleanup?.();
      return page;
    };

    void Promise.all([...keep].map(getPage)).then(async () => {
      if (!active) return;
      const page = pages.current.get(pageNumber);
      const target = canvas.current;
      const context = target?.getContext("2d");
      if (!page || !target || !context) return;
      const base = page.getViewport({ scale: 1 });
      const availableWidth = Math.max(200, (container.current?.clientWidth ?? base.width) - 24);
      const scale = fitWidth
        ? Math.min(4, Math.max(0.25, availableWidth / base.width))
        : zoom;
      const viewport = page.getViewport({ scale });
      const ratio = Math.min(2, window.devicePixelRatio || 1);
      target.width = Math.floor(viewport.width * ratio);
      target.height = Math.floor(viewport.height * ratio);
      target.style.width = `${Math.floor(viewport.width)}px`;
      target.style.height = `${Math.floor(viewport.height)}px`;
      context.setTransform(ratio, 0, 0, ratio, 0, 0);
      renderTask = page.render({ canvas: target, canvasContext: context, viewport });
      await renderTask.promise;
    }).catch((caught) => {
      if (active && (caught as { name?: string }).name !== "RenderingCancelledException") {
        setError(caught instanceof Error ? caught.message : "PDF 페이지를 표시하지 못했습니다.");
      }
    });

    return () => {
      active = false;
      renderTask?.cancel?.();
    };
  }, [document, fitWidth, pageNumber, zoom]);

  if (loading) return <div className="preview-message">PDF 불러오는 중…</div>;
  if (error) return <div className="preview-message preview-message--error" role="alert">{error}</div>;
  if (!document) return null;

  return (
    <section className="pdf-layout-view" ref={container}>
      <div className="pdf-controls" aria-label="PDF 보기 도구">
        <button
          aria-label="이전 페이지"
          disabled={pageNumber <= 1}
          onClick={() => setPageNumber((page) => Math.max(1, page - 1))}
          type="button"
        >‹</button>
        <span>{pageNumber} / {document.numPages}</span>
        <button
          aria-label="다음 페이지"
          disabled={pageNumber >= document.numPages}
          onClick={() => setPageNumber((page) => Math.min(document.numPages, page + 1))}
          type="button"
        >›</button>
        <button
          aria-label="축소"
          onClick={() => {
            setFitWidth(false);
            setZoom((value) => Math.max(0.25, value - 0.25));
          }}
          type="button"
        >−</button>
        <button
          aria-label="확대"
          onClick={() => {
            setFitWidth(false);
            setZoom((value) => Math.min(4, value + 0.25));
          }}
          type="button"
        >+</button>
        <button aria-pressed={fitWidth} onClick={() => setFitWidth(true)} type="button">
          너비 맞춤
        </button>
        <button
          aria-label="전체 화면"
          onClick={() => void container.current?.requestFullscreen?.()}
          type="button"
        >⛶</button>
      </div>
      <div className="pdf-canvas-wrap">
        <canvas aria-label={`PDF ${pageNumber}페이지`} ref={canvas} />
      </div>
    </section>
  );
}
