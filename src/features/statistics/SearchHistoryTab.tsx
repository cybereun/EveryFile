import { useEffect, useMemo, useState } from "react";
import {
  clearSearchHistory,
  deleteSearchHistory,
  listSearchHistory,
} from "../../lib/ipc";
import type { SearchFrequency, SearchHistoryRecord } from "../../lib/types";
import { useI18n } from "../../app/translations";

function formatInteger(value: string) {
  try {
    return BigInt(value).toLocaleString("ko-KR");
  } catch {
    return "0";
  }
}

function relativeTime(iso: string, locale: "ko" | "en") {
  const elapsed = Math.max(0, Date.now() - new Date(iso).getTime());
  const minute = 60_000;
  const formatter = new Intl.RelativeTimeFormat(locale === "en" ? "en-US" : "ko-KR", { numeric: "auto" });
  if (elapsed < minute) return formatter.format(0, "second");
  if (elapsed < 60 * minute) return formatter.format(-Math.floor(elapsed / minute), "minute");
  if (elapsed < 24 * 60 * minute) return formatter.format(-Math.floor(elapsed / (60 * minute)), "hour");
  return formatter.format(-Math.floor(elapsed / (24 * 60 * minute)), "day");
}

interface SearchHistoryTabProps {
  loadHistory?: () => Promise<SearchHistoryRecord[]>;
  deleteHistory?: (id: string) => Promise<boolean | void>;
  clearHistory?: () => Promise<number | void>;
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
  const { locale, t } = useI18n();
  const [history, setHistory] = useState<SearchHistoryRecord[]>([]);
  const [frequent, setFrequent] = useState(frequentSearches);
  const [view, setView] = useState<"frequent" | "recent">("frequent");
  const [message, setMessage] = useState("");

  useEffect(() => {
    let active = true;
    void loadHistory()
      .then((records) => { if (active) setHistory(records); })
      .catch(() => { if (active) setMessage(t("검색 히스토리를 불러오지 못했습니다.")); });
    return () => { active = false; };
  }, [loadHistory]);

  useEffect(() => setFrequent(frequentSearches), [frequentSearches]);

  const maximumFrequency = useMemo(
    () => frequent.reduce((maximum, item) => Math.max(maximum, Number(item.count)), 1),
    [frequent],
  );

  if (history.length === 0 && frequent.length === 0) {
    return <p role="status">{message || t("저장된 검색 히스토리가 없습니다.")}</p>;
  }

  const clearAll = async () => {
    setMessage("");
    try {
      await clearHistory();
      setHistory([]);
      setFrequent([]);
    } catch {
      setMessage(t("검색 히스토리를 삭제하지 못했습니다."));
    }
  };

  return (
    <>
      <p className="statistics-privacy-note" role="note">
        {t("비공개 검색은 기록에 저장되지 않으며 통계에도 포함되지 않습니다.")}
      </p>
      <div className="history-view-toolbar">
        <div className="history-view-switch" role="tablist" aria-label={t("검색 기록 보기")}>
          <button type="button" role="tab" aria-selected={view === "frequent"} onClick={() => setView("frequent")}>{t("자주 검색")}</button>
          <button type="button" role="tab" aria-selected={view === "recent"} onClick={() => setView("recent")}>{t("최근 검색")}</button>
        </div>
        <button className="history-clear" type="button" onClick={() => void clearAll()}>{t("전체 삭제")}</button>
      </div>

      {view === "frequent" ? (
        frequent.length > 0 ? (
          <section className="frequent-searches" aria-labelledby="frequent-searches-heading">
            <h3 className="sr-only" id="frequent-searches-heading">{t("자주 검색")}</h3>
            <ol>
              {frequent.map((item) => {
                const count = Number(item.count);
                return (
                  <li key={item.query}>
                    <button type="button" onClick={() => onSearch?.(item.query)}>{item.query}</button>
                    <span className="history-frequency-bar" aria-hidden="true"><i style={{ width: `${Math.max(10, (count / maximumFrequency) * 100)}%` }} /></span>
                    <span>{formatInteger(item.count)}{t("회")}</span>
                  </li>
                );
              })}
            </ol>
          </section>
        ) : <p role="status">{t("집계할 검색어가 없습니다.")}</p>
      ) : history.length > 0 ? (
        <ol className="recent-search-list">
          {history.map((record) => (
            <li key={record.id}>
              <button type="button" disabled={!onSearch} onClick={() => onSearch?.(record.query)}>{record.query}</button>
              <time dateTime={record.searchedAt} title={new Date(record.searchedAt).toLocaleString()}>{relativeTime(record.searchedAt, locale)}</time>
              <button type="button" aria-label={`${record.query} ${t("기록 삭제")}`} onClick={async () => {
                setMessage("");
                try {
                  const deleted = await deleteHistory(record.id);
                  if (deleted === false) {
                    setMessage(t("검색 기록이 이미 없습니다."));
                    return;
                  }
                  setHistory((records) => records.filter((item) => item.id !== record.id));
                } catch {
                  setMessage(t("검색 기록을 삭제하지 못했습니다."));
                }
              }}>×</button>
            </li>
          ))}
        </ol>
      ) : <p role="status">{t("최근 검색 기록이 없습니다.")}</p>}
      {message && <p role="alert">{message}</p>}
    </>
  );
}
