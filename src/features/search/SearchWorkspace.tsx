import {
  forwardRef,
  useEffect,
  useMemo,
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
  openSourceFile,
  searchDocuments,
} from "../../lib/ipc";
import { SearchFilters } from "./SearchFilters";
import { SearchInput } from "./SearchInput";
import { SearchResults } from "./SearchResults";
import { useImmediateSearch } from "./useImmediateSearch";

export interface SearchWorkspaceProps {
  folders: FolderRecord[];
  onSelectDocument?: (documentId: string) => void;
  searchApi?: (request: SearchRequest) => Promise<SearchResponse>;
  cancelApi?: (requestId: string) => Promise<boolean>;
  openApi?: (documentId: string) => Promise<void>;
  debounceMs?: number;
  statisticsFilter?: StatisticsSearchFilter | null;
  historyQuery?: string | null;
  pageSize?: number;
  fileClickBehavior?: "preview" | "open";
  dateDisplay?: "relative" | "absolute";
}

export const SearchWorkspace = forwardRef<HTMLInputElement, SearchWorkspaceProps>(
  function SearchWorkspace(
    {
      folders,
      onSelectDocument = () => undefined,
      searchApi = searchDocuments,
      cancelApi = cancelSearch,
      openApi = openSourceFile,
      debounceMs,
      statisticsFilter,
      historyQuery,
      pageSize,
      fileClickBehavior,
      dateDisplay,
    },
    ref: Ref<HTMLInputElement>,
  ) {
    const [withinResults, setWithinResults] = useState("");
    const search = useImmediateSearch({
      search: searchApi,
      cancel: cancelApi,
      debounceMs,
      pageSize,
    });
    const within = withinResults.trim().toLocaleLowerCase();
    useEffect(() => {
      if (!statisticsFilter) return;
      search.patchFilters(statisticsFilter);
    }, [statisticsFilter, search.patchFilters]);

    useEffect(() => {
      if (historyQuery == null) return;
      search.setQuery(historyQuery);
    }, [historyQuery, search.setQuery]);
    const visibleHits = useMemo(() => {
      if (!within) return search.hits;
      return search.hits.filter((hit) =>
        [hit.fileName, hit.path, hit.snippet ?? ""]
          .join(" ")
          .toLocaleLowerCase()
          .includes(within),
      );
    }, [search.hits, within]);

    return (
      <section className="detailed-search" role="search" aria-label="파일 검색 / File search">
        <SearchInput
          mode={search.filters.mode}
          onModeChange={(mode) => search.patchFilters({ mode })}
          onQueryChange={search.setQuery}
          query={search.query}
          ref={ref}
        />
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
          hits={visibleHits}
          loading={search.loading}
          onLoadMore={() => void search.loadMore()}
          onOpen={openApi}
          onSelect={onSelectDocument}
          total={within ? visibleHits.length : search.total}
        />
      </section>
    );
  },
);
