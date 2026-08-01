import { useEffect, useMemo, useState } from "react";
import {
  clearSearchHistory,
  deleteSearchHistory,
  listSearchHistory,
} from "../../lib/ipc";
import type { SearchFrequency, SearchHistoryRecord } from "../../lib/types";

function formatInteger(value: string) {
  try {
    return BigInt(value).toLocaleString("ko-KR");
  } catch {
    return "0";
  }
}

function relativeTime(iso: string) {
  const elapsed = Math.max(0, Date.now() - new Date(iso).getTime());
  const minute = 60_000;
  if (elapsed < minute) return "방금 전";
  if (elapsed < 60 * minute) return `${Math.floor(elapsed / minute)}분 전`;
  if (elapsed < 24 * 60 * minute) return `${Math.floor(elapsed / (60 * minute))}시간 전`;
  return `${Math.floor(elapsed / (24 * 60 * minute))}일 전`;
}

interface SearchHistoryTabProps {
  loadHistory?: () => Promise<SearchHistoryRecord[]>;
  deleteHistory?: (id: string) => Promise<void>;
  clearHistory?: () => Promise<void>;
  onSearch?: (query: string) => void;
  frequentSearches?: SearchFrequency[];
}

export function SearchHistoryTab({
  loadHistory = () => listSearchHistory(100, 0),
  deleteHistory = deleteSearchHistory,
  clearHistory = clearSearchHistory,
  onSearch,
  frequentSearches = [],
}: SearchHistoryTabProps) {
  const [history, setHistory] = useState<SearchHistoryRecord[]>([]);
  const [frequent, setFrequent] = useState(frequentSearches);
  const [view, setView] = useState<"frequent" | "recent">("frequent");
  const [message, setMessage] = useState("");

  useEffect(() => {
    let active = true;
    void loadHistory()
      .then((records) => { if (active) setHistory(records); })
      .catch(() => { if (active) setMessage("검색 히스토리를 불러오지 못했습니다."); });
    return () => { active = false; };
  }, [loadHistory]);

  useEffect(() => setFrequent(frequentSearches), [frequentSearches]);

  const maximumFrequency = useMemo(
    () => frequent.reduce((maximum, item) => Math.max(maximum, Number(item.count)), 1),
    [frequent],
  );

  if (history.length === 0 && frequent.length === 0) {
    return <p role="status">{message || "저장된 검색 히스토리가 없습니다."}</p>;
  }

  const clearAll = async () => {
    setMessage("");
    try {
      await clearHistory();
      setHistory([]);
      setFrequent([]);
    } catch {
      setMessage("검색 히스토리를 삭제하지 못했습니다.");
    }
  };

  return (
    <>
      <p className="statistics-privacy-note" role="note">
        비공개 검색은 기록에 저장되지 않으며 통계에도 포함되지 않습니다.
      </p>
      <div className="history-view-toolbar">
        <div className="history-view-switch" role="tablist" aria-label="검색 기록 보기">
          <button type="button" role="tab" aria-selected={view === "frequent"} onClick={() => setView("frequent")}>자주 검색</button>
          <button type="button" role="tab" aria-selected={view === "recent"} onClick={() => setView("recent")}>최근 검색</button>
        </div>
        <button className="history-clear" type="button" onClick={() => void clearAll()}>전체 삭제</button>
      </div>

      {view === "frequent" ? (
        frequent.length > 0 ? (
          <section className="frequent-searches" aria-labelledby="frequent-searches-heading">
            <h3 className="sr-only" id="frequent-searches-heading">자주 검색</h3>
            <ol>
              {frequent.map((item) => {
                const count = Number(item.count);
                return (
                  <li key={item.query}>
                    <button type="button" onClick={() => onSearch?.(item.query)}>{item.query}</button>
                    <span className="history-frequency-bar" aria-hidden="true"><i style={{ width: `${Math.max(10, (count / maximumFrequency) * 100)}%` }} /></span>
                    <span>{formatInteger(item.count)}회</span>
                  </li>
                );
              })}
            </ol>
          </section>
        ) : <p role="status">집계할 검색어가 없습니다.</p>
      ) : history.length > 0 ? (
        <ol className="recent-search-list">
          {history.map((record) => (
            <li key={record.id}>
              <button type="button" disabled={!onSearch} onClick={() => onSearch?.(record.query)}>{record.query}</button>
              <time dateTime={record.searchedAt} title={new Date(record.searchedAt).toLocaleString()}>{relativeTime(record.searchedAt)}</time>
              <button type="button" aria-label={`${record.query} 기록 삭제`} onClick={async () => {
                setMessage("");
                try {
                  await deleteHistory(record.id);
                  setHistory((records) => records.filter((item) => item.id !== record.id));
                } catch {
                  setMessage("검색 기록을 삭제하지 못했습니다.");
                }
              }}>×</button>
            </li>
          ))}
        </ol>
      ) : <p role="status">최근 검색 기록이 없습니다.</p>}
      {message && <p role="alert">{message}</p>}
    </>
  );
}
