import { forwardRef } from "react";
import type { SearchMode } from "../../lib/types";

interface SearchInputProps {
  query: string;
  mode: SearchMode;
  onModeChange: (mode: SearchMode) => void;
  onQueryChange: (query: string) => void;
}

function SearchIcon() {
  return (
    <svg
      aria-hidden="true"
      fill="none"
      stroke="currentColor"
      strokeLinecap="round"
      strokeWidth="2"
      viewBox="0 0 24 24"
    >
      <circle cx="11" cy="11" r="7" />
      <path d="m16 16 5 5" />
    </svg>
  );
}

export const SearchInput = forwardRef<HTMLInputElement, SearchInputProps>(
  function SearchInput({ query, mode, onModeChange, onQueryChange }, ref) {
    return (
      <div className="search-input-block">
        <div className="search-mode-tabs" aria-label="검색 대상">
          <button
            aria-pressed={mode === "keyword"}
            className={mode === "keyword" ? "is-active" : ""}
            onClick={() => onModeChange("keyword")}
            type="button"
          >
            키워드
          </button>
          <button
            aria-pressed={mode === "filename"}
            className={mode === "filename" ? "is-active" : ""}
            onClick={() => onModeChange("filename")}
            type="button"
          >
            파일명
          </button>
        </div>
        <label className="search-field search-field--workspace">
          <SearchIcon />
          <span className="sr-only">검색어</span>
          <input
            aria-label="검색어"
            onChange={(event) => onQueryChange(event.target.value)}
            placeholder="파일명이나 문서 속 단어를 입력하세요"
            ref={ref}
            type="search"
            value={query}
          />
          <kbd className="shortcut-hint" aria-hidden="true">
            /
          </kbd>
        </label>
      </div>
    );
  },
);
