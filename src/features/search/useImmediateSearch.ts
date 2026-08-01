import { useCallback, useEffect, useRef, useState } from "react";
import type {
  SearchHit,
  SearchRequest,
  SearchResponse,
} from "../../lib/types";
import {
  buildBackendQuery,
  DEFAULT_SEARCH_FILTERS,
  explicitOptionFromQuery,
  hasSearchCriteria,
  parseSearchQuery,
  queryForTermMode,
  structuredValuesFromQuery,
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
    const previous = structuredValuesFromQuery(queryRef.current);
    const next = structuredValuesFromQuery(value);
    queryRef.current = value;
    setRawQuery(value);
    setFilters((current) => {
      const patch: Partial<SearchFilters> = {};
      if (previous.extensions.length > 0 || next.extensions.length > 0) {
        patch.extensions = next.extensions;
        if (next.extensions.length > 0) patch.extensionless = false;
      }
      if (previous.modifiedAfter || next.modifiedAfter) {
        patch.modifiedAfter = next.modifiedAfter;
      }
      if (previous.modifiedBefore || next.modifiedBefore) {
        patch.modifiedBefore = next.modifiedBefore;
      }
      const explicitOption = explicitOptionFromQuery(value);
      if (explicitOption) patch.option = explicitOption;
      if (
        current.sort === "confidence" &&
        parseSearchQuery(value).positiveGroups.length === 0
      ) {
        patch.sort = "relevance";
      }
      if (Object.keys(patch).length === 0) return current;
      return { ...current, ...patch };
    });
  }, []);

  const patchFilters = useCallback((patch: Partial<SearchFilters>) => {
    if (patch.extensionless) patch.extensions = [];
    if (patch.extensions?.length) patch.extensionless = false;
    if (patch.option) {
      const normalized = queryForTermMode(queryRef.current, patch.option);
      if (normalized !== queryRef.current) {
        queryRef.current = normalized;
        setRawQuery(normalized);
      }
    }
    setFilters((current) => {
      const next = { ...current, ...patch };
      if (next.mode === "filename") {
        next.includeFilename = true;
        if (next.option === "near") next.option = "all";
        if (next.sort === "confidence") next.sort = "relevance";
      }
      return next;
    });
  }, []);

  const makeRequest = useCallback(
    (requestId: string, offset: number): SearchRequest => ({
      requestId,
      query: buildBackendQuery(query),
      mode: filters.mode,
      folderIds: filters.folderIds,
      extensions: filters.extensions,
      extensionless: filters.extensionless,
      modifiedAfter: filters.modifiedAfter,
      modifiedBefore: filters.modifiedBefore,
      includeFilename: filters.includeFilename,
      termMode: filters.option,
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

  const reset = useCallback(() => {
    latestGeneration.current += 1;
    if (activeRequestId.current) {
      void cancel(activeRequestId.current).catch(() => undefined);
      activeRequestId.current = null;
    }
    queryRef.current = "";
    setRawQuery("");
    setFilters(DEFAULT_SEARCH_FILTERS);
    setHits([]);
    setTotal(0);
    setElapsedMs(0);
    setHasMore(false);
    setLoading(false);
    setError(null);
  }, [cancel]);

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
    reset,
  };
}
