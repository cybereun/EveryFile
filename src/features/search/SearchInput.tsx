import { forwardRef } from "react";
import { useI18n } from "../../app/translations";

export type SearchWorkspaceTab = "search" | "ask";

interface SearchInputProps {
  query: string;
  onQueryChange: (query: string) => void;
  activeTab?: SearchWorkspaceTab;
  onTabChange?: (tab: SearchWorkspaceTab) => void;
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
  function SearchInput(
    {
      query,
      onQueryChange,
      activeTab = "search",
      onTabChange = () => undefined,
    },
    ref,
  ) {
    const { t } = useI18n();
    return (
      <div className="search-input-block">
        <div className="search-mode-tabs" aria-label={t("검색 방식")} role="tablist">
          <button
            aria-controls="search-workspace-panel"
            aria-selected={activeTab === "search"}
            className={`search-mode-tabs__search${activeTab === "search" ? " is-active" : ""}`}
            id="search-workspace-tab"
            onClick={() => onTabChange("search")}
            role="tab"
            type="button"
          >
            <SearchIcon /> {t("검색")}
          </button>
          <button
            aria-controls="ask-everyfile-panel"
            aria-selected={activeTab === "ask"}
            aria-label={t("Ask EveryFile")}
            className={`search-mode-tabs__ask${activeTab === "ask" ? " is-active" : ""}`}
            id="ask-everyfile-tab"
            onClick={() => onTabChange("ask")}
            role="tab"
            type="button"
          >
            <span aria-hidden="true">✦</span> {t("Ask EveryFile")}
          </button>
        </div>
        {activeTab === "search" && (
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
        )}
      </div>
    );
  },
);
