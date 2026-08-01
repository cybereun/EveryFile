import {
  Fragment,
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import type { SearchHit } from "../../lib/types";
import { exportResults } from "../../lib/ipc";
import { BrandMark } from "../../components/BrandMark";

interface SearchResultsProps {
  hits: SearchHit[];
  total: number;
  elapsedMs: number;
  loading: boolean;
  error: string | null;
  hasMore: boolean;
  onLoadMore: () => void;
  onOpen: (documentId: string) => Promise<void>;
  onSelect: (documentId: string) => void;
  clickBehavior?: "preview" | "open";
  dateDisplay?: "relative" | "absolute";
  workspaceStats?: { documents: number; folders: number };
}

function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) {
    const value = bytes / 1024;
    return `${Number.isInteger(value) ? value : value.toFixed(1)} KB`;
  }
  const value = bytes / (1024 * 1024);
  return `${Number.isInteger(value) ? value : value.toFixed(1)} MB`;
}

function formatBreadcrumbPath(path: string) {
  const normalized = path.replace(/^\\\\\?\\/, "");
  return normalized
    .split(/[\\/]+/)
    .filter(Boolean)
    .join(" / ");
}

function parseDate(value: string) {
  const trimmed = value.trim();
  if (/^\d+$/.test(trimmed)) {
    try {
      const raw = BigInt(trimmed);
      // Index records created by older builds used Unix nanoseconds. Accept
      // seconds/milliseconds too so upgraded libraries render consistently.
      const milliseconds =
        trimmed.length >= 16
          ? raw / 1_000_000n
          : trimmed.length >= 13
            ? raw
            : raw * 1_000n;
      const parsed = new Date(Number(milliseconds));
      if (Number.isFinite(parsed.getTime())) return parsed;
    } catch {
      // Fall through to the normal ISO parser below.
    }
  }
  const parsed = new Date(trimmed);
  return Number.isFinite(parsed.getTime()) ? parsed : null;
}

function relativeDate(value: string) {
  const parsed = parseDate(value);
  if (!parsed) return "날짜 없음";
  const elapsed = parsed.getTime() - Date.now();
  const units = [
    ["year", 365 * 24 * 60 * 60 * 1000],
    ["month", 30 * 24 * 60 * 60 * 1000],
    ["day", 24 * 60 * 60 * 1000],
    ["hour", 60 * 60 * 1000],
    ["minute", 60 * 1000],
  ] as const;
  const formatter = new Intl.RelativeTimeFormat("ko-KR", { numeric: "auto" });
  for (const [unit, milliseconds] of units) {
    if (Math.abs(elapsed) >= milliseconds) {
      return formatter.format(Math.round(elapsed / milliseconds), unit);
    }
  }
  return "방금";
}

function absoluteDate(value: string) {
  return parseDate(value)?.toLocaleString("ko-KR") ?? "날짜 없음";
}

export function highlightedSnippet(text: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  const marker = /<\/?mark>/g;
  let cursor = 0;
  let highlighted = false;
  let match: RegExpExecArray | null;

  while ((match = marker.exec(text))) {
    const token = match[0];
    if ((token === "<mark>" && highlighted) || (token === "</mark>" && !highlighted)) {
      continue;
    }
    const value = text.slice(cursor, match.index);
    if (value) {
      nodes.push(
        highlighted ? <mark key={nodes.length}>{value}</mark> : value,
      );
    }
    highlighted = token === "<mark>";
    cursor = match.index + token.length;
  }
  const remainder = text.slice(cursor);
  if (remainder) {
    nodes.push(
      highlighted ? <mark key={nodes.length}>{remainder}</mark> : remainder,
    );
  }
  return nodes;
}

function ResultRow({
  hit,
  selected,
  onSelect,
  onOpen,
  clickBehavior,
  dateDisplay,
}: {
  hit: SearchHit;
  selected: boolean;
  onSelect: () => void;
  onOpen: () => void;
  clickBehavior: "preview" | "open";
  dateDisplay: "relative" | "absolute";
}) {
  const parent = hit.path.replace(/[\\/][^\\/]+$/, "");
  return (
    <button
      aria-selected={selected}
      className={`search-result-row${selected ? " is-selected" : ""}`}
      id={`search-result-${hit.documentId}`}
      onClick={() => {
        onSelect();
        if (clickBehavior === "open") onOpen();
      }}
      onDoubleClick={onOpen}
      role="option"
      type="button"
    >
      <span className="result-heading">
        <strong>{hit.fileName}</strong>
        <span className="extension-badge">{hit.extension.toUpperCase()}</span>
      </span>
      <span className="result-meta">
        <span className="result-path" title={parent}>
          {formatBreadcrumbPath(parent)}
        </span>
        <time dateTime={parseDate(hit.modifiedAt)?.toISOString()}>
          {dateDisplay === "relative"
            ? relativeDate(hit.modifiedAt)
            : absoluteDate(hit.modifiedAt)}
        </time>
        <span>{formatSize(hit.sizeBytes)}</span>
      </span>
      {hit.snippet && (
        <span className="result-snippet">{highlightedSnippet(hit.snippet)}</span>
      )}
    </button>
  );
}

export function SearchResults({
  hits,
  total,
  elapsedMs,
  loading,
  error,
  hasMore,
  onLoadMore,
  onOpen,
  onSelect,
  clickBehavior = "preview",
  dateDisplay = "absolute",
  workspaceStats = { documents: 0, folders: 0 },
}: SearchResultsProps) {
  const [selectedDocumentId, setSelectedDocumentId] = useState<string | null>(
    hits[0]?.documentId ?? null,
  );
  const selectedDocumentIdRef = useRef(selectedDocumentId);
  const [openError, setOpenError] = useState<string | null>(null);

  const selectDocument = useCallback(
    (documentId: string | null, notify = true) => {
      selectedDocumentIdRef.current = documentId;
      setSelectedDocumentId(documentId);
      if (documentId && notify) onSelect(documentId);
    },
    [onSelect],
  );

  useEffect(() => {
    if (
      selectedDocumentId &&
      hits.some((hit) => hit.documentId === selectedDocumentId)
    ) {
      selectedDocumentIdRef.current = selectedDocumentId;
      return;
    }
    const next = hits[0]?.documentId ?? null;
    selectDocument(next);
  }, [hits, selectDocument, selectedDocumentId]);

  const openDocument = async (documentId: string | null) => {
    if (!documentId) return;
    setOpenError(null);
    try {
      await onOpen(documentId);
    } catch (caught) {
      setOpenError(caught instanceof Error ? caught.message : "파일을 열지 못했습니다.");
    }
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (!hits.length) return;
    const current = Math.max(
      0,
      hits.findIndex((hit) => hit.documentId === selectedDocumentIdRef.current),
    );
    if (event.key === "ArrowDown") {
      event.preventDefault();
      const next = hits[Math.min(hits.length - 1, current + 1)];
      selectDocument(next.documentId);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      const next = hits[Math.max(0, current - 1)];
      selectDocument(next.documentId);
    } else if (event.key === "Enter") {
      event.preventDefault();
      void openDocument(selectedDocumentIdRef.current);
    }
  };

  if (error) return <div className="search-message search-message--error">{error}</div>;
  if (!loading && hits.length === 0) {
    return (
      <div className="workspace-empty">
        <div className="workspace-empty__content">
          <BrandMark className="brand-mark--hero" />
          <strong>EveryFile<span aria-hidden="true">.</span></strong>
          <p>내 PC 깊숙이 흩어진 문서들.<br />이제 빠르게 찾아보세요.</p>
          <span className="workspace-empty__stats">
            {workspaceStats.documents.toLocaleString()} 문서 · {workspaceStats.folders.toLocaleString()} 폴더
          </span>
        </div>
      </div>
    );
  }

  return (
    <section className="search-results" aria-label="검색 결과 영역">
      <div className="results-summary" aria-live="polite">
        <strong>{total.toLocaleString()}개</strong>
        <span>{elapsedMs.toLocaleString()}ms</span>
        {loading && <span>검색 중…</span>}
        {hits.length > 0 && (
          <span className="results-export">
            <button
              type="button"
              onClick={() =>
                void exportResults({ kind: "searchResults", hits }, "csv").catch(
                  () => setOpenError("CSV 내보내기에 실패했습니다."),
                )
              }
            >
              CSV
            </button>
            <button
              type="button"
              onClick={() =>
                void exportResults({ kind: "searchResults", hits }, "xlsx").catch(
                  () => setOpenError("Excel 내보내기에 실패했습니다."),
                )
              }
            >
              Excel
            </button>
          </span>
        )}
      </div>
      <div
        aria-activedescendant={
          selectedDocumentId ? `search-result-${selectedDocumentId}` : undefined
        }
        aria-label="검색 결과"
        className="search-results-list"
        onKeyDown={handleKeyDown}
        role="listbox"
        tabIndex={0}
      >
        {hits.map((hit, index) => {
          const previousKind = hits[index - 1]?.matchKind;
          const labels = {
            filename: "파일명 일치",
            content: "내용 일치",
            both: "파일명·내용 일치",
            metadata: "필터 일치",
          };
          return (
            <Fragment key={hit.documentId}>
              {previousKind !== hit.matchKind && (
                <h3 className="result-group-heading">{labels[hit.matchKind]}</h3>
              )}
              <ResultRow
                clickBehavior={clickBehavior}
                dateDisplay={dateDisplay}
                hit={hit}
                onOpen={() => {
                  selectDocument(hit.documentId);
                  void openDocument(hit.documentId);
                }}
                onSelect={() => {
                  selectDocument(hit.documentId);
                }}
                selected={selectedDocumentId === hit.documentId}
              />
            </Fragment>
          );
        })}
      </div>
      {openError && (
        <div className="search-message search-message--error" role="alert">
          {openError}
        </div>
      )}
      {hasMore && (
        <button className="load-more" disabled={loading} onClick={onLoadMore} type="button">
          결과 더 보기
        </button>
      )}
    </section>
  );
}
