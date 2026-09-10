import {
  useCallback,
  forwardRef,
  useEffect,
  useRef,
  useState,
  type Ref,
} from "react";
import type {
  FolderRecord,
  SearchRequest,
  SearchResponse,
  StatisticsSearchFilter,
} from "../../lib/types";
import {
  cancelSearch,
  cancelDocumentAi,
  listSearchHistory,
  openSourceFile,
  openSourceLocation,
  runDocumentAi,
  searchDocuments,
} from "../../lib/ipc";
import { useI18n } from "../../app/translations";
import { DocumentAiPanel } from "../preview/DocumentAiPanel";
import { SearchFilters } from "./SearchFilters";
import { SearchInput, type SearchWorkspaceTab } from "./SearchInput";
import { SearchResults } from "./SearchResults";
import { useImmediateSearch } from "./useImmediateSearch";

export interface SearchWorkspaceProps {
  folders: FolderRecord[];
  onAddFolder?: () => void;
  onSelectDocument?: (documentId: string, query: string) => void;
  searchApi?: (request: SearchRequest) => Promise<SearchResponse>;
  cancelApi?: (requestId: string) => Promise<boolean>;
  openApi?: (documentId: string) => Promise<void>;
  openLocationApi?: (documentId: string) => Promise<void>;
  debounceMs?: number;
  statisticsFilter?: StatisticsSearchFilter | null;
  historyQuery?: string | null;
  homeRequest?: number;
  pageSize?: number;
  fileClickBehavior?: "preview" | "open";
  dateDisplay?: "relative" | "absolute";
  aiEnabled?: boolean;
  aiProvider?: "ollama" | "gemini" | "openai";
  selectedDocumentId?: string | null;
  onOpenAiSettings?: () => void;
  runAiApi?: typeof runDocumentAi;
  cancelAiApi?: typeof cancelDocumentAi;
  onHistoryChanged?: () => void;
  historyRequest?: number;
  focusSearchRequest?: number;
}

export const SearchWorkspace = forwardRef<HTMLInputElement, SearchWorkspaceProps>(
  function SearchWorkspace(
    {
      folders,
      onAddFolder,
      onSelectDocument = () => undefined,
      searchApi = searchDocuments,
      cancelApi = cancelSearch,
      openApi = openSourceFile,
      openLocationApi = openSourceLocation,
      debounceMs,
      statisticsFilter,
      historyQuery,
      homeRequest = 0,
      pageSize,
      fileClickBehavior,
      dateDisplay,
      aiEnabled = false,
      aiProvider = "ollama",
      selectedDocumentId = null,
      onOpenAiSettings,
      runAiApi = runDocumentAi,
      cancelAiApi = cancelDocumentAi,
      onHistoryChanged,
      historyRequest = 0,
      focusSearchRequest = 0,
    },
    ref: Ref<HTMLInputElement>,
  ) {
    const { t } = useI18n();
    const [withinResults, setWithinResults] = useState("");
    const [activeTab, setActiveTab] = useState<SearchWorkspaceTab>("search");
    const inputRef = useRef<HTMLInputElement>(null);
    const setInputRef = useCallback(
      (node: HTMLInputElement | null) => {
        inputRef.current = node;
        if (typeof ref === "function") ref(node);
        else if (ref) ref.current = node;
      },
      [ref],
    );
    const search = useImmediateSearch({
      search: searchApi,
      cancel: cancelApi,
      debounceMs,
      pageSize,
      withinQuery: withinResults,
    });
    useEffect(() => {
      if (!statisticsFilter) return;
      search.patchFilters(statisticsFilter);
    }, [statisticsFilter, search.patchFilters]);

    useEffect(() => {
      if (historyQuery == null) return;
      search.setQuery(historyQuery);
    }, [historyQuery, historyRequest, search.setQuery]);

    useEffect(() => {
      if (homeRequest > 0) {
        setActiveTab("search");
        setWithinResults("");
        search.reset();
      }
    }, [homeRequest, search.reset]);

    useEffect(() => {
      if (focusSearchRequest <= 0) return;
      setActiveTab("search");
      const focus = window.requestAnimationFrame(() => inputRef.current?.focus());
      return () => window.cancelAnimationFrame(focus);
    }, [focusSearchRequest]);

    useEffect(() => {
      if (search.loading || !search.query.trim()) return;
      const refresh = window.setTimeout(() => {
        void listSearchHistory(5, 0)
          .then(() => onHistoryChanged?.())
          .catch(() => undefined);
      }, 250);
      return () => window.clearTimeout(refresh);
    }, [onHistoryChanged, search.hits.length, search.loading, search.query]);

    return (
      <section className="detailed-search" role="search" aria-label="파일 검색 / File search">
        <SearchInput
          activeTab={activeTab}
          onQueryChange={search.setQuery}
          onTabChange={setActiveTab}
          query={search.query}
          ref={setInputRef}
        />
        {activeTab === "search" ? (
          <div
            aria-labelledby="search-workspace-tab"
            id="search-workspace-panel"
            role="tabpanel"
          >
            <SearchFilters
              filters={search.filters}
              folders={folders}
              query={search.query}
              onFiltersChange={search.patchFilters}
              onQueryChange={search.setQuery}
              onWithinResultsChange={setWithinResults}
              withinResults={withinResults}
            />
            <SearchResults
              clickBehavior={fileClickBehavior}
              dateDisplay={dateDisplay}
              elapsedMs={search.elapsedMs}
              error={search.error}
              hasMore={search.hasMore}
              hits={search.hits}
              loading={search.loading}
              onLoadMore={() => void search.loadMore()}
              onOpen={openApi}
              onOpenLocation={openLocationApi}
              onSelect={(documentId) => onSelectDocument(documentId, search.query)}
              total={search.total}
              onAddFolder={onAddFolder}
            />
          </div>
        ) : (
          <section
            aria-labelledby="ask-everyfile-tab"
            className="ask-everyfile-panel"
            id="ask-everyfile-panel"
            role="tabpanel"
          >
            {!aiEnabled ? (
              <div className="ask-everyfile-empty">
                <span aria-hidden="true" className="ask-everyfile-empty__mark">✦</span>
                <h2>{t("Ask EveryFile")}</h2>
                <p>{t("AI 기능을 켜면 선택한 파일에 대해 질문할 수 있습니다.")}</p>
                <button
                  className="primary-button"
                  onClick={onOpenAiSettings}
                  type="button"
                >
                  {t("AI 설정 열기")}
                </button>
              </div>
            ) : !selectedDocumentId ? (
              <div className="ask-everyfile-empty">
                <span aria-hidden="true" className="ask-everyfile-empty__mark">✦</span>
                <h2>{t("질문할 파일을 선택하세요.")}</h2>
                <p>{t("검색 탭에서 문서를 선택한 뒤 Ask EveryFile로 돌아오세요.")}</p>
                <button onClick={() => setActiveTab("search")} type="button">
                  {t("검색으로 돌아가기")}
                </button>
              </div>
            ) : (
              <div className="ask-everyfile-question">
                <header>
                  <h2>{t("Ask EveryFile")}</h2>
                  <p>{t("선택한 파일에 대해 AI에게 질문하세요.")}</p>
                </header>
                <DocumentAiPanel
                  cancelApi={cancelAiApi}
                  documentId={selectedDocumentId}
                  mode="question"
                  onClose={() => setActiveTab("search")}
                  provider={aiProvider}
                  runApi={runAiApi}
                />
              </div>
            )}
          </section>
        )}
      </section>
    );
  },
);
