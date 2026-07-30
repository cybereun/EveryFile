import { useEffect, useState } from "react";
import {
  clearSearchHistory,
  deleteSearchHistory,
  listSearchHistory,
} from "../../lib/ipc";
import type { SearchFrequency, SearchHistoryRecord } from "../../lib/types";

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
  const [message, setMessage] = useState("");

  useEffect(() => {
    let active = true;
    void loadHistory()
      .then((records) => {
        if (active) setHistory(records);
      })
      .catch(() => {
        if (active) setMessage("검색 히스토리를 불러오지 못했습니다.");
      });
    return () => {
      active = false;
    };
  }, [loadHistory]);

  if (history.length === 0 && frequentSearches.length === 0) {
    return <p role="status">{message || "저장된 검색 히스토리가 없습니다."}</p>;
  }

  return (
    <>
      {frequentSearches.length > 0 && (
        <section className="frequent-searches" aria-labelledby="frequent-searches-heading">
          <h3 id="frequent-searches-heading">자주 검색</h3>
          <ol>
            {frequentSearches.map((item) => (
              <li key={item.query}>
                <button type="button" onClick={() => onSearch?.(item.query)}>
                  {item.query}
                </button>
                <span>{item.count.toLocaleString()}회</span>
              </li>
            ))}
          </ol>
        </section>
      )}
      {history.length > 0 ? (
        <>
          <div className="history-toolbar">
            <strong>최근 검색</strong>
            <span>{history.length.toLocaleString()}개 검색</span>
            <button
              type="button"
              onClick={async () => {
                await clearHistory();
                setHistory([]);
              }}
            >
              전체 삭제
            </button>
          </div>
          <table className="data-table" aria-label="검색 히스토리">
        <thead>
          <tr>
            <th scope="col">검색어</th>
            <th scope="col">결과</th>
            <th scope="col">검색 시각</th>
            <th scope="col">작업</th>
          </tr>
        </thead>
        <tbody>
          {history.map((record) => (
            <tr key={record.id}>
              <th scope="row">
                <button
                  className="table-link"
                  type="button"
                  disabled={!onSearch}
                  onClick={() => onSearch?.(record.query)}
                >
                  {record.query}
                </button>
              </th>
              <td>{record.resultCount.toLocaleString()}</td>
              <td>
                <time dateTime={record.searchedAt}>
                  {new Date(record.searchedAt).toLocaleString()}
                </time>
              </td>
              <td>
                <button
                  type="button"
                  aria-label={`${record.query} 기록 삭제`}
                  onClick={async () => {
                    await deleteHistory(record.id);
                    setHistory((records) =>
                      records.filter((item) => item.id !== record.id),
                    );
                  }}
                >
                  삭제
                </button>
              </td>
            </tr>
          ))}
        </tbody>
          </table>
        </>
      ) : (
        <p role="status">최근 검색 기록이 없습니다.</p>
      )}
    </>
  );
}
