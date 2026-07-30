import { useCallback, useEffect, useRef, useState } from "react";
import type {
  SearchHit,
  SearchRequest,
  SearchResponse,
} from "../../lib/types";
import {
  buildBackendQuery,
  DEFAULT_SEARCH_FILTERS,
  extensionsFromQuery,
  hasSearchCriteria,
  type SearchFilters,
} from "./searchStore";

export interface ImmediateSearchApi {
  search: (request: SearchRequest) => Promise<SearchResponse>;
  cancel: (requestId: string) => Promise<boolean>;
}

export interface UseImmediateSearchOptions extends ImmediateSearchApi {
  debounceMs?: number;
  pageSize?: number;
  initialFilters?: SearchFilters;
}

function nextRequestId(sequence: number) {
  return `search-${Date.now().toString(36)}-${sequence.toString(36)}`;
}

export function useImmediateSearch({
  search,
  cancel,
  debounceMs = 120,
  pageSize = 100,
  initialFilters = DEFAULT_SEARCH_FILTERS,
}: UseImmediateSearchOptions) {
  const [query, setRawQuery] = useState("");
  const [filters, setFilters] = useState<SearchFilters>(initialFilters);
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [total, setTotal] = useState(0);
  const [elapsedMs, setElapsedMs] = useState(0);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const sequence = useRef(0);
  const queryRef = useRef("");
  const activeRequestId = useRef<string | null>(null);
  const latestGeneration = useRef(0);

  const setQuery = useCallback((value: string) => {
    const previousExtensions = extensionsFromQuery(queryRef.current);
    queryRef.current = value;
    setRawQuery(value);
    const extensions = extensionsFromQuery(value);
    if (previousExtensions.length === 0 && extensions.length === 0) return;
    setFilters((current) => {
      if (
        current.extensions.length === extensions.length &&
        current.extensions.every((extension, index) => extension === extensions[index])
      ) {
        return current;
      }
      return { ...current, extensions };
    });
  }, []);

  const patchFilters = useCallback((patch: Partial<SearchFilters>) => {
    setFilters((current) => ({ ...current, ...patch }));
  }, []);

  const makeRequest = useCallback(
    (requestId: string, offset: number): SearchRequest => ({
      requestId,
      query: buildBackendQuery(query, filters.option),
      mode: filters.mode,
      folderIds: filters.folderIds,
      extensions: filters.extensions,
      modifiedAfter: filters.modifiedAfter,
      modifiedBefore: filters.modifiedBefore,
      includeFilename: filters.includeFilename,
      privateSearch: false,
      sort: filters.sort,
      limit: pageSize,
      offset,
    }),
    [filters, pageSize, query],
  );

  const execute = useCallback(
    async (generation: number, offset: number, append: boolean) => {
      const previousRequestId = activeRequestId.current;
      const requestId = nextRequestId(++sequence.current);
      activeRequestId.current = requestId;
      if (previousRequestId) {
        void cancel(previousRequestId).catch(() => undefined);
      }
      setLoading(true);
      setError(null);
      try {
        const response = await search(makeRequest(requestId, offset));
        if (
          generation !== latestGeneration.current ||
          response.requestId !== requestId
        ) {
          return;
        }
        setHits((current) => (append ? [...current, ...response.hits] : response.hits));
        setTotal(response.total);
        setElapsedMs(response.elapsedMs);
        setHasMore(response.hasMore);
      } catch (caught) {
        if (generation !== latestGeneration.current) return;
        const code =
          typeof caught === "object" && caught && "code" in caught
            ? String(caught.code)
            : "";
        if (code !== "SEARCH_CANCELLED") {
          setError(caught instanceof Error ? caught.message : "검색하지 못했습니다.");
        }
      } finally {
        if (generation === latestGeneration.current) setLoading(false);
        if (activeRequestId.current === requestId) activeRequestId.current = null;
      }
    },
    [cancel, makeRequest, search],
  );

  useEffect(() => {
    const generation = ++latestGeneration.current;
    if (!hasSearchCriteria(query, filters)) {
      const previousRequestId = activeRequestId.current;
      activeRequestId.current = null;
      if (previousRequestId) void cancel(previousRequestId).catch(() => undefined);
      setHits([]);
      setTotal(0);
      setHasMore(false);
      setLoading(false);
      return;
    }

    const timer = window.setTimeout(() => {
      void execute(generation, 0, false);
    }, debounceMs);
    return () => window.clearTimeout(timer);
  }, [cancel, debounceMs, execute, filters, query]);

  useEffect(
    () => () => {
      latestGeneration.current += 1;
      if (activeRequestId.current) {
        void cancel(activeRequestId.current).catch(() => undefined);
      }
    },
    [cancel],
  );

  const loadMore = useCallback(async () => {
    if (loading || !hasMore) return;
    const generation = ++latestGeneration.current;
    await execute(generation, hits.length, true);
  }, [execute, hasMore, hits.length, loading]);

  return {
    query,
    setQuery,
    filters,
    patchFilters,
    hits,
    total,
    elapsedMs,
    hasMore,
    loading,
    error,
    loadMore,
  };
}
