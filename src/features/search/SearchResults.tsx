import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import type { SearchHit } from "../../lib/types";
import { exportResults } from "../../lib/ipc";
import { useI18n } from "../../app/translations";

interface SearchResultsProps {
  hits: SearchHit[];
  total: number;
  elapsedMs: number;
  loading: boolean;
  error: string | null;
  hasMore: boolean;
  onLoadMore: () => void;
  onOpen: (documentId: string) => Promise<void>;
  onOpenLocation?: (documentId: string) => Promise<void>;
  onSelect: (documentId: string) => void;
  onAddFolder?: () => void;
  clickBehavior?: "preview" | "open";
  dateDisplay?: "relative" | "absolute";
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

function relativeDate(value: string, locale: "ko" | "en") {
  const parsed = parseDate(value);
  if (!parsed) return locale === "en" ? "Unknown date" : "날짜 없음";
  const elapsed = parsed.getTime() - Date.now();
  const units = [
    ["year", 365 * 24 * 60 * 60 * 1000],
    ["month", 30 * 24 * 60 * 60 * 1000],
    ["day", 24 * 60 * 60 * 1000],
    ["hour", 60 * 60 * 1000],
    ["minute", 60 * 1000],
  ] as const;
  const formatter = new Intl.RelativeTimeFormat(locale === "en" ? "en-US" : "ko-KR", { numeric: "auto" });
  for (const [unit, milliseconds] of units) {
    if (Math.abs(elapsed) >= milliseconds) {
      return formatter.format(Math.round(elapsed / milliseconds), unit);
    }
  }
  return formatter.format(0, "second");
}

function absoluteDate(value: string, locale: "ko" | "en") {
  return parseDate(value)?.toLocaleString(locale === "en" ? "en-US" : "ko-KR") ?? (locale === "en" ? "Unknown date" : "날짜 없음");
}

const MATCH_GROUP_ORDER: SearchHit["matchKind"][] = [
  "content",
  "both",
  "filename",
  "metadata",
];

const MATCH_GROUP_LABELS: Record<SearchHit["matchKind"], string> = {
  filename: "파일명 일치",
  content: "내용 일치",
  both: "파일명·내용 일치",
  metadata: "필터 일치",
};

function CopyPathIcon() {
  return (
    <svg aria-hidden="true" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.7" viewBox="0 0 24 24">
      <path d="M9 8V5.5A2.5 2.5 0 0 1 11.5 3H20v8.5a2.5 2.5 0 0 1-2.5 2.5H15" />
      <rect height="11" rx="2" width="11" x="3" y="10" />
      <path d="M7 13.5h3m-3 3h4" />
    </svg>
  );
}

function FolderOpenIcon() {
  return (
    <svg aria-hidden="true" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.7" viewBox="0 0 24 24">
      <path d="M3 7.5A2.5 2.5 0 0 1 5.5 5H10l2 2h6.5A2.5 2.5 0 0 1 21 9.5v1" />
      <path d="M3 8.5v9A2.5 2.5 0 0 0 5.5 20h11.7a2.5 2.5 0 0 0 2.3-1.5l1.5-5A1.9 1.9 0 0 0 19.2 11H6.3a2.2 2.2 0 0 0-2.1 1.5L3 16" />
    </svg>
  );
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
  onOpenLocation,
  onCopyPath,
  onContextMenu,
  compareSelected,
  clickBehavior,
  dateDisplay,
  locale,
  t,
}: {
  hit: SearchHit;
  selected: boolean;
  onSelect: () => void;
  onOpen: () => void;
  onOpenLocation: () => void;
  onCopyPath: () => void;
  onContextMenu: (event: React.MouseEvent<HTMLDivElement>) => void;
  compareSelected: boolean;
  clickBehavior: "preview" | "open";
  dateDisplay: "relative" | "absolute";
  locale: "ko" | "en";
  t: (source: string) => string;
}) {
  const parent = hit.path.replace(/[\\/][^\\/]+$/, "");
  return (
    <div
      aria-selected={selected}
      className={`search-result-row${selected ? " is-selected" : ""}${compareSelected ? " is-compare-target" : ""}`}
      id={`search-result-${hit.documentId}`}
      onClick={() => {
        onSelect();
        if (clickBehavior === "open") onOpen();
      }}
      onDoubleClick={onOpen}
      onContextMenu={onContextMenu}
      role="option"
    >
      <span className="result-heading">
        <strong>{hit.fileName}</strong>
        <span className="extension-badge">{hit.extension.toUpperCase()}</span>
        {hit.matchKind === "both" && <span className="match-count-badge">2{t("개")} 매칭</span>}
        {compareSelected && <span className="compare-target-badge">{t("비교 대상으로 선택")}</span>}
        <span className="result-actions">
          <button
            aria-label={`${hit.fileName} ${t("경로 복사")}`}
            className="result-action"
            onClick={(event) => {
              event.stopPropagation();
              onCopyPath();
            }}
            title={t("경로 복사")}
            type="button"
          >
            <CopyPathIcon />
          </button>
          <button
            aria-label={`${hit.fileName} ${t("파일 위치 열기")}`}
            className="result-action"
            onClick={(event) => {
              event.stopPropagation();
              onOpenLocation();
            }}
            title={t("파일 위치 열기")}
            type="button"
          >
            <FolderOpenIcon />
          </button>
        </span>
      </span>
      <span className="result-meta">
        <span className="result-path" title={parent}>
          {formatBreadcrumbPath(parent)}
        </span>
        <time dateTime={parseDate(hit.modifiedAt)?.toISOString()}>
          {dateDisplay === "relative"
            ? relativeDate(hit.modifiedAt, locale)
            : absoluteDate(hit.modifiedAt, locale)}
        </time>
        <span>{formatSize(hit.sizeBytes)}</span>
      </span>
      {hit.snippet && (
        <span className="result-snippet">{highlightedSnippet(hit.snippet)}</span>
      )}
    </div>
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
  onOpenLocation = async () => undefined,
  onSelect,
  onAddFolder,
  clickBehavior = "preview",
  dateDisplay = "absolute",
}: SearchResultsProps) {
  const { locale, t } = useI18n();
  const groupedHits = useMemo(() => {
    const groups = new Map<SearchHit["matchKind"], SearchHit[]>();
    for (const hit of hits) {
      const group = groups.get(hit.matchKind) ?? [];
      group.push(hit);
      groups.set(hit.matchKind, group);
    }
    return MATCH_GROUP_ORDER.flatMap((kind) => groups.get(kind) ?? []);
  }, [hits]);
  const [selectedDocumentId, setSelectedDocumentId] = useState<string | null>(null);
  const selectedDocumentIdRef = useRef(selectedDocumentId);
  const [openError, setOpenError] = useState<string | null>(null);
  const [actionNotice, setActionNotice] = useState<string | null>(null);
  const [contextMenu, setContextMenu] = useState<{
    documentId: string;
    x: number;
    y: number;
  } | null>(null);
  const [compareTargetId, setCompareTargetId] = useState<string | null>(null);
  const contextMenuRef = useRef<HTMLDivElement>(null);

  const selectDocument = useCallback(
    (documentId: string | null, notify = true) => {
      selectedDocumentIdRef.current = documentId;
      setSelectedDocumentId(documentId);
      if (documentId && notify) onSelect(documentId);
    },
    [onSelect],
  );

  useEffect(() => {
    const activeDocumentId = selectedDocumentIdRef.current;
    if (activeDocumentId && groupedHits.some((hit) => hit.documentId === activeDocumentId)) {
      if (selectedDocumentId !== activeDocumentId) setSelectedDocumentId(activeDocumentId);
      return;
    }
    const next = groupedHits[0]?.documentId ?? null;
    selectDocument(next);
  }, [groupedHits, selectDocument, selectedDocumentId]);

  useEffect(() => {
    if (!contextMenu) return;
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (!contextMenuRef.current?.contains(event.target as Node)) {
        setContextMenu(null);
      }
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setContextMenu(null);
    };
    document.addEventListener("pointerdown", closeOnOutsidePointer);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("pointerdown", closeOnOutsidePointer);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [contextMenu]);

  const openDocument = async (documentId: string | null) => {
    if (!documentId) return;
    setOpenError(null);
    try {
      await onOpen(documentId);
    } catch (caught) {
      setOpenError(caught instanceof Error ? caught.message : t("파일을 열지 못했습니다."));
    }
  };

  const openLocation = async (documentId: string | null) => {
    if (!documentId) return;
    setOpenError(null);
    try {
      await onOpenLocation(documentId);
    } catch (caught) {
      setOpenError(caught instanceof Error ? caught.message : t("파일 위치를 열지 못했습니다."));
    }
  };

  const copyPath = async (hit: SearchHit) => {
    setOpenError(null);
    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(hit.path);
      } else {
        const textarea = document.createElement("textarea");
        textarea.value = hit.path;
        textarea.setAttribute("readonly", "true");
        textarea.style.position = "fixed";
        textarea.style.opacity = "0";
        document.body.appendChild(textarea);
        textarea.select();
        const copied = document.execCommand("copy");
        textarea.remove();
        if (!copied) throw new Error("clipboard unavailable");
      }
      setActionNotice(t("경로를 클립보드에 복사했습니다."));
      window.setTimeout(() => setActionNotice(null), 2200);
    } catch {
      setOpenError(t("경로를 복사하지 못했습니다."));
    }
  };

  const showContextMenu = (event: React.MouseEvent<HTMLDivElement>, hit: SearchHit) => {
    event.preventDefault();
    selectDocument(hit.documentId);
    setContextMenu({ documentId: hit.documentId, x: event.clientX, y: event.clientY });
  };

  const runContextAction = (action: () => void) => {
    setContextMenu(null);
    action();
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (!groupedHits.length) return;
    if (event.ctrlKey && event.key.toLocaleLowerCase() === "c") {
      const selectedHit = groupedHits.find(
        (hit) => hit.documentId === selectedDocumentIdRef.current,
      );
      if (selectedHit) {
        event.preventDefault();
        void copyPath(selectedHit);
      }
      return;
    }
    const current = Math.max(
      0,
      groupedHits.findIndex((hit) => hit.documentId === selectedDocumentIdRef.current),
    );
    if (event.key === "ArrowDown") {
      event.preventDefault();
      const next = groupedHits[Math.min(groupedHits.length - 1, current + 1)];
      selectDocument(next.documentId);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      const next = groupedHits[Math.max(0, current - 1)];
      selectDocument(next.documentId);
    } else if (event.key === "Enter") {
      event.preventDefault();
      void openDocument(selectedDocumentIdRef.current);
    }
  };

  if (error) return <div className="search-message search-message--error">{error}</div>;
  if (!loading && groupedHits.length === 0) {
    return (
      <div className="workspace-empty">
        <div className="workspace-empty__content">
          <button
            className="workspace-empty__folder-register"
            type="button"
            onClick={onAddFolder}
            disabled={!onAddFolder}
            aria-label={t("폴더 등록")}
          >
            <svg aria-hidden="true" fill="none" viewBox="0 0 48 48">
              <path d="M6 14a4 4 0 0 1 4-4h11l4 5h13a4 4 0 0 1 4 4v17a4 4 0 0 1-4 4H10a4 4 0 0 1-4-4V14Z" />
              <path d="M24 20v14m-7-7h14" />
            </svg>
          </button>
          <p>{t("폴더를 등록하면 문서를 검색할 수 있습니다.")}</p>
        </div>
      </div>
    );
  }

  return (
    <section className="search-results" aria-label={t("검색 결과 영역")}>
      <div className="results-summary" aria-live="polite">
        <strong>{total.toLocaleString()}{t("개")}</strong>
        <span>{elapsedMs.toLocaleString()}ms</span>
        {loading && <span>{t("검색 중…")}</span>}
        {groupedHits.length > 0 && (
          <span className="results-export">
            <button
              type="button"
              onClick={() =>
                void exportResults({ kind: "searchResults", hits: groupedHits }, "csv").catch(
                  () => setOpenError(t("CSV 내보내기에 실패했습니다.")),
                )
              }
            >
              CSV
            </button>
            <button
              type="button"
              onClick={() =>
                void exportResults({ kind: "searchResults", hits: groupedHits }, "xlsx").catch(
                  () => setOpenError(t("Excel 내보내기에 실패했습니다.")),
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
        aria-label={t("검색 결과")}
        className="search-results-list"
        onKeyDown={handleKeyDown}
        role="listbox"
        tabIndex={0}
      >
        {groupedHits.map((hit, index) => {
          const previousKind = groupedHits[index - 1]?.matchKind;
          return (
            <div className="search-result-item" key={hit.documentId}>
              {previousKind !== hit.matchKind && (
                <h3 className={`result-group-heading${index > 0 ? " is-continuation" : ""}`}>
                  {t(MATCH_GROUP_LABELS[hit.matchKind])}
                </h3>
              )}
              <ResultRow
                clickBehavior={clickBehavior}
                dateDisplay={dateDisplay}
                hit={hit}
                compareSelected={compareTargetId === hit.documentId}
                onOpen={() => {
                  selectDocument(hit.documentId);
                  void openDocument(hit.documentId);
                }}
                onOpenLocation={() => void openLocation(hit.documentId)}
                onCopyPath={() => void copyPath(hit)}
                onContextMenu={(event) => showContextMenu(event, hit)}
          onSelect={() => {
            selectDocument(hit.documentId);
          }}
                locale={locale}
                t={t}
                selected={selectedDocumentId === hit.documentId}
              />
            </div>
          );
        })}
      </div>
      {contextMenu && (() => {
        const contextHit = groupedHits.find((hit) => hit.documentId === contextMenu.documentId);
        if (!contextHit) return null;
        const left = Math.min(contextMenu.x, Math.max(8, window.innerWidth - 300));
        const top = Math.min(contextMenu.y, Math.max(8, window.innerHeight - 300));
        return (
          <div
            aria-label={t("검색 결과 메뉴")}
            className="search-result-context-menu"
            ref={contextMenuRef}
            role="menu"
            style={{ left, top }}
          >
            <button aria-label={t("파일 열기")} onClick={() => runContextAction(() => void openDocument(contextHit.documentId))} role="menuitem" type="button">
              <span aria-hidden="true">↗</span><span>{t("파일 열기")}</span><kbd>Enter</kbd>
            </button>
            <button aria-label={t("파일 위치 열기")} onClick={() => runContextAction(() => void openLocation(contextHit.documentId))} role="menuitem" type="button">
              <span aria-hidden="true"><FolderOpenIcon /></span><span>{t("파일 위치 열기")}</span><span />
            </button>
            <button aria-label={t("경로 복사")} onClick={() => runContextAction(() => void copyPath(contextHit))} role="menuitem" type="button">
              <span aria-hidden="true"><CopyPathIcon /></span><span>{t("경로 복사")}</span><kbd>Ctrl+C</kbd>
            </button>
            <div className="search-result-context-menu__separator" role="separator" />
            <button aria-label={t("유사 문서 찾기")} disabled role="menuitem" type="button">
              <span aria-hidden="true">⌕</span><span>{t("유사 문서 찾기")}</span><small>{t("시맨틱 OFF")}</small>
            </button>
            <button aria-label={t("비교 대상으로 선택")} onClick={() => runContextAction(() => {
              setCompareTargetId(contextHit.documentId);
              setActionNotice(t("비교 대상으로 선택했습니다."));
              window.setTimeout(() => setActionNotice(null), 2200);
            })} role="menuitem" type="button">
              <span aria-hidden="true">⌘</span><span>{t("비교 대상으로 선택")}</span><span />
            </button>
          </div>
        );
      })()}
      {openError && (
        <div className="search-message search-message--error" role="alert">
          {openError}
        </div>
      )}
      {actionNotice && <div className="search-action-notice" role="status">{actionNotice}</div>}
      {hasMore && (
        <button className="load-more" disabled={loading} onClick={onLoadMore} type="button">
          {t("결과 더 보기")}
        </button>
      )}
    </section>
  );
}
