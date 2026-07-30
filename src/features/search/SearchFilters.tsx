import { useState } from "react";
import type { FolderRecord } from "../../lib/types";
import {
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
}: SearchFiltersProps) {
  const [extensionOpen, setExtensionOpen] = useState(false);
  const [dateOpen, setDateOpen] = useState(false);
  const [presetSaved, setPresetSaved] = useState(false);

  const toggleExtension = (extension: string) => {
    const extensions = filters.extensions.includes(extension)
      ? filters.extensions.filter((value) => value !== extension)
      : [...filters.extensions, extension];
    onQueryChange(withExtensionQuery(query, extensions));
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
              <option value="near">인접 검색</option>
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
              <option value="confidence">신뢰도순</option>
              <option value="newest">최신순</option>
              <option value="oldest">오래된순</option>
              <option value="name">이름순</option>
              <option value="size">크기순</option>
            </select>
          </label>
          <div className="filter-popover">
            <button
              aria-expanded={extensionOpen}
              onClick={() => setExtensionOpen((open) => !open)}
              type="button"
            >
              확장자
            </button>
            <div className="filter-menu" hidden={!extensionOpen}>
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
            </div>
          </div>
          <div className="filter-popover">
            <button
              aria-expanded={dateOpen}
              onClick={() => setDateOpen((open) => !open)}
              type="button"
            >
              기간
            </button>
            <div className="filter-menu filter-menu--date" hidden={!dateOpen}>
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
            </div>
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
              onChange={(event) =>
                onFiltersChange({ withinResults: event.target.value })
              }
              placeholder="결과 내 검색…"
              type="text"
              value={filters.withinResults}
            />
          </label>
          <button onClick={savePreset} type="button">
            {presetSaved ? "저장됨" : "프리셋 저장"}
          </button>
        </div>
      </div>
      <FilterChips
        filters={filters}
        folders={folders}
        query={query}
        onFiltersChange={onFiltersChange}
        onQueryChange={onQueryChange}
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
}: SearchFiltersProps) {
  const selectedFolder = folders.find((folder) =>
    filters.folderIds.includes(folder.id),
  );

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
      {(filters.modifiedAfter || filters.modifiedBefore) && (
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
    </div>
  );
}
