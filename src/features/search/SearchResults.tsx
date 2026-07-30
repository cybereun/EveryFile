import {
  Fragment,
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import type { SearchHit } from "../../lib/types";

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
}: {
  hit: SearchHit;
  selected: boolean;
  onSelect: () => void;
  onOpen: () => void;
}) {
  const parent = hit.path.replace(/[\\/][^\\/]+$/, "");
  return (
    <button
      aria-selected={selected}
      className={`search-result-row${selected ? " is-selected" : ""}`}
      id={`search-result-${hit.documentId}`}
      onClick={onSelect}
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
          {parent}
        </span>
        <time dateTime={hit.modifiedAt}>
          {new Date(hit.modifiedAt).toLocaleString("ko-KR")}
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
        <div>
          <strong>Anything in your files.</strong>
          <p>검색어와 필터를 선택하면 이곳에 결과가 표시됩니다.</p>
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
