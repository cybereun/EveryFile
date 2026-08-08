import { forwardRef } from "react";
import { useI18n } from "../../app/translations";
interface SearchInputProps {
  query: string;
  onQueryChange: (query: string) => void;
  aiEnabled?: boolean;
  onAskEveryfile?: () => void;
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
  function SearchInput({ query, onQueryChange, aiEnabled = false, onAskEveryfile }, ref) {
    const { t } = useI18n();
    return (
      <div className="search-input-block">
        <div className="search-mode-tabs" aria-label={t("검색 방식")}>
          <button
            aria-pressed="true"
            className="is-active search-mode-tabs__search"
            type="button"
          >
            <SearchIcon /> {t("검색")}
          </button>
          <button
            aria-label="Ask Everyfile"
            aria-disabled={!aiEnabled}
            className={`search-mode-tabs__ask${aiEnabled ? "" : " is-disabled"}`}
            onClick={onAskEveryfile}
            type="button"
          >
            <span aria-hidden="true">✦</span> Ask Everyfile
          </button>
        </div>
        <label className="search-field search-field--workspace">
          <SearchIcon />
          <span className="sr-only">{t("검색어")}</span>
          <input
            aria-label={t("검색어")}
            onChange={(event) => onQueryChange(event.target.value)}
            placeholder={t("파일명이나 문서 속 단어를 입력하세요")}
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
