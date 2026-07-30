import { Fragment, useEffect, useMemo, useState, type ReactNode } from "react";
import type { SearchHit } from "../../lib/types";

interface SearchResultsProps {
  hits: SearchHit[];
  query: string;
  total: number;
  elapsedMs: number;
  loading: boolean;
  error: string | null;
  hasMore: boolean;
  onLoadMore: () => void;
  onOpen: (documentId: string) => Promise<void>;
  onSelect: (documentId: string) => void;
}

function filenameMatchesQuery(hit: SearchHit, query: string) {
  if (!hit.snippet) return true;
  const terms = query
    .replace(/\b(?:ext|path|after|before):(?:"[^"]*"|\S+)/gi, " ")
    .match(/"([^"]+)"|[^\s]+/g)
    ?.map((term) => term.replace(/^"|"$/g, "").replace(/^-/, ""))
    .filter((term) => term && term !== "OR" && !term.startsWith("~"));
  const fileName = hit.fileName.toLocaleLowerCase();
  return Boolean(
    terms?.some((term) => fileName.includes(term.toLocaleLowerCase())),
  );
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
  index,
  onSelect,
  onOpen,
}: {
  hit: SearchHit;
  selected: boolean;
  index: number;
  onSelect: () => void;
  onOpen: () => void;
}) {
  const parent = hit.path.replace(/[\\/][^\\/]+$/, "");
  return (
    <button
      aria-selected={selected}
      className={`search-result-row${selected ? " is-selected" : ""}`}
      id={`search-result-${index}`}
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
  query,
  total,
  elapsedMs,
  loading,
  error,
  hasMore,
  onLoadMore,
  onOpen,
  onSelect,
}: SearchResultsProps) {
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [openError, setOpenError] = useState<string | null>(null);
  const groups = useMemo(
    () => [
      {
        label: "파일명 일치",
        hits: hits.filter((hit) => filenameMatchesQuery(hit, query)),
      },
      {
        label: "내용 일치",
        hits: hits.filter((hit) => !filenameMatchesQuery(hit, query)),
      },
    ],
    [hits, query],
  );
  const orderedHits = useMemo(
    () => groups.flatMap((group) => group.hits),
    [groups],
  );

  useEffect(() => {
    setSelectedIndex((current) =>
      Math.min(current, Math.max(0, orderedHits.length - 1)),
    );
  }, [orderedHits.length]);

  const openSelected = async () => {
    const hit = orderedHits[selectedIndex];
    if (!hit) return;
    setOpenError(null);
    try {
      await onOpen(hit.documentId);
    } catch (caught) {
      setOpenError(caught instanceof Error ? caught.message : "파일을 열지 못했습니다.");
    }
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (!orderedHits.length) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      const next = Math.min(orderedHits.length - 1, selectedIndex + 1);
      setSelectedIndex(next);
      onSelect(orderedHits[next].documentId);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      const next = Math.max(0, selectedIndex - 1);
      setSelectedIndex(next);
      onSelect(orderedHits[next].documentId);
    } else if (event.key === "Enter") {
      event.preventDefault();
      void openSelected();
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

  let absoluteIndex = -1;
  return (
    <section className="search-results" aria-label="검색 결과 영역">
      <div className="results-summary" aria-live="polite">
        <strong>{total.toLocaleString()}개</strong>
        <span>{elapsedMs.toLocaleString()}ms</span>
        {loading && <span>검색 중…</span>}
      </div>
      <div
        aria-activedescendant={
          orderedHits[selectedIndex] ? `search-result-${selectedIndex}` : undefined
        }
        aria-label="검색 결과"
        className="search-results-list"
        onKeyDown={handleKeyDown}
        role="listbox"
        tabIndex={0}
      >
        {groups.map((group) => {
          if (group.hits.length === 0) return null;
          return (
            <Fragment key={group.label}>
              <h3 className="result-group-heading">{group.label}</h3>
              {group.hits.map((hit) => {
                const index = ++absoluteIndex;
                return (
                  <ResultRow
                    hit={hit}
                    index={index}
                    key={hit.documentId}
                    onOpen={() => {
                      setSelectedIndex(index);
                      void onOpen(hit.documentId);
                    }}
                    onSelect={() => {
                      setSelectedIndex(index);
                      onSelect(hit.documentId);
                    }}
                    selected={selectedIndex === index}
                  />
                );
              })}
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
