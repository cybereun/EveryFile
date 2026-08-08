import initRhwp, { HwpDocument } from "@rhwp/core";
import wasmUrl from "@rhwp/core/rhwp_bg.wasm?url";
import { useEffect, useMemo, useRef, useState } from "react";
import { cancelPdfRead, getLayoutBytes } from "../../lib/ipc";
import { useI18n, type Translator } from "../../app/translations";

let rhwpReady: Promise<unknown> | null = null;
let requestSequence = 0;

function ensureRhwp() {
  if (!rhwpReady) {
    const scope = globalThis as typeof globalThis & {
      measureTextWidth?: (font: string, text: string) => number;
    };
    let context: CanvasRenderingContext2D | null = null;
    let lastFont = "";
    scope.measureTextWidth = (font, text) => {
      context ??= document.createElement("canvas").getContext("2d");
      if (!context) return text.length * 8;
      if (lastFont !== font) {
        context.font = font;
        lastFont = font;
      }
      return context.measureText(text).width;
    };
    rhwpReady = initRhwp({ module_or_path: wasmUrl });
  }
  return rhwpReady;
}

export interface HwpDocumentLike {
  pageCount(): number;
  renderPageSvg(pageNumber: number): string;
  free(): void;
}

export type HwpLoader = (bytes: Uint8Array) => Promise<HwpDocumentLike>;

async function loadHwp(bytes: Uint8Array): Promise<HwpDocumentLike> {
  await ensureRhwp();
  return new HwpDocument(bytes);
}

function nextRequestId() {
  requestSequence += 1;
  return `hwp-layout-${Date.now().toString(36)}-${requestSequence.toString(36)}`;
}

function safeMessage(caught: unknown, t: Translator) {
  const code = caught && typeof caught === "object" && "code" in caught ? String(caught.code) : "";
  if (code === "SOURCE_LAYOUT_UNSUPPORTED") return t("이 파일은 원본 레이아웃으로 표시할 수 없습니다.");
  if (code === "SOURCE_PDF_TOO_LARGE") return t("문서가 원본 미리보기 크기 제한을 초과했습니다.");
  if (/CANCELLED/.test(code)) return null;
  return caught instanceof Error && caught.message
    ? caught.message
    : t("HWP 원본 레이아웃을 불러오지 못했습니다.");
}

function sanitizeAndHighlightSvg(raw: string, query: string, activeMatch: number) {
  const parsed = new DOMParser().parseFromString(raw, "image/svg+xml");
  const svg = parsed.documentElement;
  if (svg.tagName.toLocaleLowerCase() !== "svg" || parsed.querySelector("parsererror")) {
    throw new Error("HWP_RENDER_INVALID_SVG");
  }
  parsed.querySelectorAll("script,foreignObject,iframe,object,embed").forEach((node) => node.remove());
  parsed.querySelectorAll("*").forEach((element) => {
    for (const attribute of [...element.attributes]) {
      const name = attribute.name.toLocaleLowerCase();
      if (name.startsWith("on")) element.removeAttribute(attribute.name);
      if ((name === "href" || name.endsWith(":href")) &&
          !attribute.value.startsWith("#") &&
          !attribute.value.startsWith("data:image/")) {
        element.removeAttribute(attribute.name);
      }
    }
  });

  const target = query.trim().toLocaleLowerCase();
  let matchIndex = 0;
  if (target) {
    parsed.querySelectorAll("text,tspan").forEach((element) => {
      const textNodes = [...element.childNodes].filter((node) => node.nodeType === Node.TEXT_NODE);
      for (const textNode of textNodes) {
        const text = textNode.textContent ?? "";
        const source = text.toLocaleLowerCase();
        if (!source.includes(target)) continue;
        const fragment = parsed.createDocumentFragment();
        let cursor = 0;
        let found = source.indexOf(target);
        while (found !== -1) {
          if (found > cursor) fragment.append(text.slice(cursor, found));
          const highlight = parsed.createElementNS("http://www.w3.org/2000/svg", "tspan");
          highlight.setAttribute(
            "class",
            `hwp-search-match${matchIndex === activeMatch ? " is-active" : ""}`,
          );
          highlight.setAttribute("data-match-index", String(matchIndex));
          highlight.textContent = text.slice(found, found + query.trim().length);
          fragment.append(highlight);
          matchIndex += 1;
          cursor = found + query.trim().length;
          found = source.indexOf(target, cursor);
        }
        fragment.append(text.slice(cursor));
        textNode.replaceWith(fragment);
      }
    });
  }
  return { svg: new XMLSerializer().serializeToString(svg), matches: matchIndex };
}

interface HwpLayoutViewProps {
  documentId: string;
  initialQuery?: string;
  findRequest?: number;
  getBytesApi?: typeof getLayoutBytes;
  cancelReadApi?: typeof cancelPdfRead;
  loader?: HwpLoader;
}

export function HwpLayoutView({
  documentId,
  initialQuery = "",
  findRequest = 0,
  getBytesApi = getLayoutBytes,
  cancelReadApi = cancelPdfRead,
  loader = loadHwp,
}: HwpLayoutViewProps) {
  const { t } = useI18n();
  const container = useRef<HTMLElement>(null);
  const [hwp, setHwp] = useState<HwpDocumentLike | null>(null);
  const [pageNumber, setPageNumber] = useState(1);
  const [zoom, setZoom] = useState(1);
  const [fitWidth, setFitWidth] = useState(true);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [query, setQuery] = useState(initialQuery);
  const [findOpen, setFindOpen] = useState(Boolean(initialQuery.trim()));
  const [activeMatch, setActiveMatch] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (findRequest > 0) setFindOpen(true);
  }, [findRequest]);

  useEffect(() => {
    setQuery(initialQuery);
    setFindOpen(Boolean(initialQuery.trim()));
    setActiveMatch(0);
  }, [initialQuery, documentId]);

  useEffect(() => {
    const requestId = nextRequestId();
    let active = true;
    let documentHandle: HwpDocumentLike | null = null;
    setLoading(true);
    setError(null);
    setHwp(null);
    setPageNumber(1);
    void getBytesApi(documentId, requestId)
      .then((bytes) => loader(new Uint8Array(bytes)))
      .then((loaded) => {
        if (!active) return;
        documentHandle = loaded;
        setHwp(documentHandle);
      })
      .catch((caught) => {
        if (!active) return;
        const message = safeMessage(caught, t);
        if (message) setError(message);
      })
      .finally(() => { if (active) setLoading(false); });
    return () => {
      active = false;
      void cancelReadApi(requestId).catch(() => undefined);
      documentHandle?.free();
    };
  }, [cancelReadApi, documentId, getBytesApi, loader, t]);

  const rendered = useMemo(() => {
    if (!hwp) return { svg: "", matches: 0 };
    try {
      return sanitizeAndHighlightSvg(hwp.renderPageSvg(pageNumber - 1), query, activeMatch);
    } catch (caught) {
      return { svg: "", matches: 0, error: safeMessage(caught, t) ?? t("원본 페이지 렌더링에 실패했습니다.") };
    }
  }, [activeMatch, hwp, pageNumber, query, t]);

  useEffect(() => {
    const active = container.current?.querySelector(".hwp-search-match.is-active");
    active?.scrollIntoView?.({ block: "center", inline: "center" });
  }, [activeMatch, pageNumber, query]);

  useEffect(() => {
    const handleFullscreenChange = () => {
      setIsFullscreen(document.fullscreenElement === container.current);
    };
    document.addEventListener("fullscreenchange", handleFullscreenChange);
    handleFullscreenChange();
    return () => document.removeEventListener("fullscreenchange", handleFullscreenChange);
  }, []);

  const toggleFullscreen = async () => {
    const element = container.current;
    if (!element) return;
    try {
      if (document.fullscreenElement === element) {
        await document.exitFullscreen();
      } else {
        await element.requestFullscreen();
      }
    } catch {
      setIsFullscreen(false);
    }
  };

  if (loading) return <div className="preview-message">{t("HWP 원본 불러오는 중…")}</div>;
  const renderError = "error" in rendered ? rendered.error : null;
  if (error || renderError) {
    return <div className="preview-message preview-message--error" role="alert">{error ?? renderError}</div>;
  }
  if (!hwp) return null;
  const pageCount = Math.max(1, hwp.pageCount());
  const moveMatch = (direction: number) => {
    if (!rendered.matches) return;
    setActiveMatch((current) => (current + direction + rendered.matches) % rendered.matches);
  };

  return (
    <section className="hwp-layout-view" ref={container}>
      <div className="pdf-controls" aria-label={t("HWP 보기 도구")}>
        <button aria-label={t("이전 페이지")} disabled={pageNumber <= 1} onClick={() => setPageNumber((page) => Math.max(1, page - 1))} type="button">‹</button>
        <span>{pageNumber} / {pageCount}</span>
        <button aria-label={t("다음 페이지")} disabled={pageNumber >= pageCount} onClick={() => setPageNumber((page) => Math.min(pageCount, page + 1))} type="button">›</button>
        <button aria-label={t("축소")} onClick={() => { setFitWidth(false); setZoom((value) => Math.max(0.5, value - 0.1)); }} type="button">−</button>
        <button aria-pressed={fitWidth} onClick={() => setFitWidth(true)} type="button">{t("맞춤")}</button>
        <button aria-label={t("확대")} onClick={() => { setFitWidth(false); setZoom((value) => Math.min(3, value + 0.1)); }} type="button">+</button>
        <button
          aria-label={isFullscreen ? t("원래 크기로") : t("전체 화면")}
          aria-pressed={isFullscreen}
          onClick={() => void toggleFullscreen()}
          type="button"
        >
          {isFullscreen ? "⤢" : "⛶"}
        </button>
      </div>
      {findOpen && (
        <div className="document-find hwp-layout-find">
          <input aria-label={t("원본에서 찾기")} onChange={(event) => { setQuery(event.target.value); setActiveMatch(0); }} placeholder={t("원본에서 찾기")} type="search" value={query} />
          <span aria-live="polite">{rendered.matches ? `${activeMatch + 1} / ${rendered.matches}` : "0 / 0"}</span>
          <button aria-label={t("이전 일치")} onClick={() => moveMatch(-1)} type="button">↑</button>
          <button aria-label={t("다음 일치")} onClick={() => moveMatch(1)} type="button">↓</button>
          <button aria-label={t("찾기 닫기")} onClick={() => setFindOpen(false)} type="button">×</button>
        </div>
      )}
      <div className="hwp-page-wrap">
        <div
          aria-label={`HWP ${pageNumber}${t("페이지")}`}
          className={`hwp-page${fitWidth ? " is-fit" : ""}`}
          dangerouslySetInnerHTML={{ __html: rendered.svg }}
          style={fitWidth ? undefined : { width: `${zoom * 100}%` }}
        />
      </div>
    </section>
  );
}
