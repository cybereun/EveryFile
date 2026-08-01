import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react";
import { createPortal } from "react-dom";
import type { FolderRecord } from "../../lib/types";
import {
  parseSearchQuery,
  queryForTermMode,
  removeQueryClause,
  withExtensionQuery,
  type SearchFilters as SearchFilterState,
} from "./searchStore";

const EXTENSIONS = ["hwpx", "hwp", "docx", "pptx", "xlsx", "pdf", "txt"];

interface SearchFiltersProps {
  filters: SearchFilterState;
  folders: FolderRecord[];
  query: string;
  onFiltersChange: (patch: Partial<SearchFilterState>) => void;
  onQueryChange: (query: string) => void;
  withinResults: string;
  onWithinResultsChange: (query: string) => void;
}

function AnchoredPopover({
  anchor,
  label,
  open,
  width,
  onClose,
  children,
  className = "",
}: {
  anchor: RefObject<HTMLButtonElement | null>;
  label: string;
  open: boolean;
  width: number;
  onClose: () => void;
  children: ReactNode;
  className?: string;
}) {
  const menu = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: 8, top: 8 });

  useLayoutEffect(() => {
    if (!open || !anchor.current) return;
    const update = () => {
      const rectangle = anchor.current?.getBoundingClientRect();
      if (!rectangle) return;
      const left = Math.max(8, Math.min(rectangle.left, window.innerWidth - width - 8));
      const estimatedHeight = menu.current?.offsetHeight || 280;
      const below = rectangle.bottom + 6;
      const top =
        below + estimatedHeight <= window.innerHeight
          ? below
          : Math.max(8, rectangle.top - estimatedHeight - 6);
      setPosition({ left, top });
    };
    update();
    window.addEventListener("resize", update);
    window.addEventListener("scroll", update, true);
    return () => {
      window.removeEventListener("resize", update);
      window.removeEventListener("scroll", update, true);
    };
  }, [anchor, open, width]);

  useEffect(() => {
    if (!open) return;
    const dismiss = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!menu.current?.contains(target) && !anchor.current?.contains(target)) onClose();
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
        anchor.current?.focus();
      }
    };
    document.addEventListener("pointerdown", dismiss);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("pointerdown", dismiss);
      document.removeEventListener("keydown", escape);
    };
  }, [anchor, onClose, open]);

  if (!open) return null;
  return createPortal(
    <div
      aria-label={label}
      className={`filter-menu filter-menu--portal ${className}`}
      ref={menu}
      role="dialog"
      style={{ left: position.left, top: position.top, width }}
    >
      {children}
    </div>,
    document.body,
  );
}

function dateString(date: Date) {
  return [
    date.getFullYear(),
    String(date.getMonth() + 1).padStart(2, "0"),
    String(date.getDate()).padStart(2, "0"),
  ].join("-");
}

function presetDates(days: number) {
  const before = new Date();
  const after = new Date(before);
  after.setDate(after.getDate() - days);
  return { modifiedAfter: dateString(after), modifiedBefore: dateString(before) };
}

export function SearchFilters({
  filters,
  folders,
  query,
  onFiltersChange,
  onQueryChange,
  withinResults,
  onWithinResultsChange,
}: SearchFiltersProps) {
  const [extensionOpen, setExtensionOpen] = useState(false);
  const [dateOpen, setDateOpen] = useState(false);
  const [presetSaved, setPresetSaved] = useState(false);
  const extensionAnchor = useRef<HTMLButtonElement>(null);
  const dateAnchor = useRef<HTMLButtonElement>(null);
  const hasPositiveQuery =
    parseSearchQuery(query).positiveGroups.length > 0;

  useEffect(() => {
    try {
      window.localStorage.setItem(
        "everyfile.search.current",
        JSON.stringify({ query, filters }),
      );
    } catch {
      // Search remains usable when browser storage is unavailable.
    }
  }, [filters, query]);

  const toggleExtension = (extension: string) => {
    const extensions = filters.extensions.includes(extension)
      ? filters.extensions.filter((value) => value !== extension)
      : [...filters.extensions, extension];
    onQueryChange(withExtensionQuery(query, extensions));
    onFiltersChange({ extensionless: false });
  };

  const savePreset = () => {
    try {
      window.localStorage.setItem(
        "everyfile.search.preset",
        JSON.stringify({ query, filters }),
      );
    } catch {
      // The current search remains usable if hardened storage is unavailable.
    }
    setPresetSaved(true);
    window.setTimeout(() => setPresetSaved(false), 1200);
  };

  return (
    <>
      <div
        className="search-filter-scroller"
        data-testid="search-filter-scroller"
        style={{ overflowX: "auto" }}
      >
        <div className="search-filter-row">
          <label>
            <span className="sr-only">검색 옵션</span>
            <select
              aria-label="검색 옵션"
              onChange={(event) =>
                onFiltersChange({
                  option: event.target.value as SearchFilterState["option"],
                })
              }
              value={filters.option}
            >
              <option value="all">모두 포함</option>
              <option value="any">하나 이상</option>
              <option value="exact">정확히 일치</option>
              <option value="exclude">제외 검색</option>
              <option disabled={filters.mode === "filename"} value="near">
                인접 검색 (문서 내용 전용)
              </option>
            </select>
          </label>
          <label>
            <span className="sr-only">정렬</span>
            <select
              aria-label="정렬"
              onChange={(event) =>
                onFiltersChange({
                  sort: event.target.value as SearchFilterState["sort"],
                })
              }
              value={filters.sort}
            >
              <option value="relevance">관련도순</option>
              <option
                disabled={filters.mode === "filename" || !hasPositiveQuery}
                value="confidence"
              >
                신뢰도순 (문서 내용 전용)
              </option>
              <option value="newest">최신순</option>
              <option value="oldest">오래된순</option>
              <option value="name">이름순</option>
              <option value="size">크기순</option>
            </select>
          </label>
          <div className="filter-popover">
            <button
              aria-expanded={extensionOpen}
              onClick={() => {
                setExtensionOpen((open) => !open);
                setDateOpen(false);
              }}
              ref={extensionAnchor}
              type="button"
            >
              확장자
            </button>
            <AnchoredPopover
              anchor={extensionAnchor}
              label="확장자 필터"
              onClose={() => setExtensionOpen(false)}
              open={extensionOpen}
              width={176}
            >
              {EXTENSIONS.map((extension) => (
                <label key={extension}>
                  <input
                    checked={filters.extensions.includes(extension)}
                    onChange={() => toggleExtension(extension)}
                    type="checkbox"
                  />
                  {extension.toUpperCase()}
                </label>
              ))}
              <label>
                <input
                  checked={filters.extensionless}
                  onChange={(event) => {
                    onFiltersChange({
                      extensionless: event.target.checked,
                      extensions: event.target.checked ? [] : filters.extensions,
                    });
                    if (event.target.checked) {
                      onQueryChange(withExtensionQuery(query, []));
                    }
                  }}
                  type="checkbox"
                />
                확장자 없음
              </label>
            </AnchoredPopover>
          </div>
          <div className="filter-popover">
            <button
              aria-expanded={dateOpen}
              onClick={() => {
                setDateOpen((open) => !open);
                setExtensionOpen(false);
              }}
              ref={dateAnchor}
              type="button"
            >
              기간
            </button>
            <AnchoredPopover
              anchor={dateAnchor}
              className="filter-menu--date"
              label="기간 필터"
              onClose={() => setDateOpen(false)}
              open={dateOpen}
              width={256}
            >
              <button
                onClick={() => onFiltersChange(presetDates(0))}
                type="button"
              >
                오늘
              </button>
              {[7, 30, 90, 180, 365].map((days) => (
                <button
                  key={days}
                  onClick={() => onFiltersChange(presetDates(days))}
                  type="button"
                >
                  {days === 180 ? "6개월" : days === 365 ? "1년" : `${days}일`}
                </button>
              ))}
              <div className="custom-date-range">
                <label>
                  시작일
                  <input
                    aria-label="시작일"
                    max={filters.modifiedBefore ?? undefined}
                    onChange={(event) =>
                      onFiltersChange({
                        modifiedAfter: event.target.value || null,
                      })
                    }
                    type="date"
                    value={filters.modifiedAfter ?? ""}
                  />
                </label>
                <label>
                  종료일
                  <input
                    aria-label="종료일"
                    min={filters.modifiedAfter ?? undefined}
                    onChange={(event) =>
                      onFiltersChange({
                        modifiedBefore: event.target.value || null,
                      })
                    }
                    type="date"
                    value={filters.modifiedBefore ?? ""}
                  />
                </label>
              </div>
            </AnchoredPopover>
          </div>
          <label>
            <span className="sr-only">폴더 범위</span>
            <select
              aria-label="폴더 범위"
              onChange={(event) =>
                onFiltersChange({
                  folderIds: event.target.value ? [event.target.value] : [],
                })
              }
              value={filters.folderIds[0] ?? ""}
            >
              <option value="">전체 폴더</option>
              {folders.map((folder) => (
                <option key={folder.id} value={folder.id}>
                  {folder.displayName}
                </option>
              ))}
            </select>
          </label>
          <label className="filter-check">
            <input
              aria-label="파일명 포함"
              checked={filters.includeFilename}
              disabled={filters.mode === "filename"}
              onChange={(event) =>
                onFiltersChange({ includeFilename: event.target.checked })
              }
              type="checkbox"
            />
            파일명 포함
          </label>
          <label className="within-results">
            <span className="sr-only">결과 내 검색</span>
            <input
              aria-label="결과 내 검색"
              onChange={(event) => onWithinResultsChange(event.target.value)}
              placeholder="결과 내 검색…"
              type="text"
              value={withinResults}
            />
          </label>
          <button onClick={savePreset} type="button">
            {presetSaved ? "저장됨" : "프리셋 저장"}
          </button>
        </div>
      </div>
      {filters.mode === "filename" && (
        <p className="filter-context-note" role="status">
          파일명 검색에서는 파일명이 항상 포함되며 인접·신뢰도 검색은 문서 내용
          모드에서만 사용할 수 있습니다.
        </p>
      )}
      <FilterChips
        filters={filters}
        folders={folders}
        query={query}
        onFiltersChange={onFiltersChange}
        onQueryChange={onQueryChange}
        onWithinResultsChange={onWithinResultsChange}
        withinResults={withinResults}
      />
    </>
  );
}

function FilterChips({
  filters,
  folders,
  query,
  onFiltersChange,
  onQueryChange,
  withinResults,
  onWithinResultsChange,
}: SearchFiltersProps) {
  const selectedFolder = folders.find((folder) =>
    filters.folderIds.includes(folder.id),
  );
  const parsed = parseSearchQuery(query);
  const queryHasDate = parsed.clauses.some(
    (clause) => clause.kind === "after" || clause.kind === "before",
  );
  const optionLabels: Record<SearchFilterState["option"], string> = {
    all: "모두 포함",
    any: "하나 이상",
    exact: "정확히 일치",
    exclude: "제외 검색",
    near: "인접 검색",
  };

  const clearOption = () => {
    onQueryChange(queryForTermMode(query, "all"));
    onFiltersChange({ option: "all" });
  };

  return (
    <div className="filter-chips" aria-label="적용된 필터">
      {filters.extensions.map((extension) => (
        <button
          aria-label={`확장자 ${extension.toUpperCase()} 제거`}
          key={extension}
          onClick={() =>
            onQueryChange(
              withExtensionQuery(
                query,
                filters.extensions.filter((value) => value !== extension),
              ),
            )
          }
          type="button"
        >
          {extension.toUpperCase()} <span aria-hidden="true">×</span>
        </button>
      ))}
      {filters.extensionless && (
        <button
          aria-label="확장자 없음 필터 제거"
          onClick={() => onFiltersChange({ extensionless: false })}
          type="button"
        >
          확장자 없음 <span aria-hidden="true">×</span>
        </button>
      )}
      {(filters.modifiedAfter || filters.modifiedBefore) && !queryHasDate && (
        <button
          aria-label="기간 필터 제거"
          onClick={() =>
            onFiltersChange({ modifiedAfter: null, modifiedBefore: null })
          }
          type="button"
        >
          {filters.modifiedAfter ?? "…"} – {filters.modifiedBefore ?? "…"}{" "}
          <span aria-hidden="true">×</span>
        </button>
      )}
      {parsed.clauses
        .filter((clause) =>
          ["path", "after", "before"].includes(clause.kind),
        )
        .map((clause, index) => (
          <button
            aria-label={`${clause.raw} 제거`}
            key={`${clause.kind}-${clause.raw}-${index}`}
            onClick={() => {
              const nextQuery = removeQueryClause(
                query,
                (candidate) =>
                  candidate.kind === clause.kind && candidate.raw === clause.raw,
              );
              onQueryChange(nextQuery);
            }}
            type="button"
          >
            {clause.raw} <span aria-hidden="true">×</span>
          </button>
        ))}
      {selectedFolder && (
        <button
          aria-label={`폴더 ${selectedFolder.displayName} 제거`}
          onClick={() => onFiltersChange({ folderIds: [] })}
          type="button"
        >
          {selectedFolder.displayName} <span aria-hidden="true">×</span>
        </button>
      )}
      {!filters.includeFilename && (
        <button
          aria-label="파일명 제외 필터 제거"
          onClick={() => onFiltersChange({ includeFilename: true })}
          type="button"
        >
          파일명 제외 <span aria-hidden="true">×</span>
        </button>
      )}
      {filters.option !== "all" && (
        <button
          aria-label={`${optionLabels[filters.option]} 옵션 제거`}
          onClick={clearOption}
          type="button"
        >
          {optionLabels[filters.option]} <span aria-hidden="true">×</span>
        </button>
      )}
      {withinResults && (
        <button
          aria-label="결과 내 검색 제거"
          onClick={() => onWithinResultsChange("")}
          type="button"
        >
          결과 내: {withinResults} <span aria-hidden="true">×</span>
        </button>
      )}
    </div>
  );
}
