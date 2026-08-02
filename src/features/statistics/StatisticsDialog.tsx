import { useCallback, useEffect, useState } from "react";
import { useModalDialog } from "../../components/useModalDialog";
import {
  deleteSearchHistory,
  getStatistics,
  listSearchHistory,
} from "../../lib/ipc";
import type {
  DocumentStatistics,
  SearchHistoryRecord,
  StatisticsBucket,
  StatisticsSearchFilter,
} from "../../lib/types";
import { SearchHistoryTab } from "./SearchHistoryTab";

type StatisticsTab = "documents" | "history";

const CHART_COLORS = [
  "#df8f7c", "#327ab8", "#d8ab3e", "#6f9f77", "#7257ad",
  "#49a7b5", "#c96d55", "#87964c", "#9a6c91", "#7a6042",
];

export interface StatisticsDialogProps {
  open: boolean;
  onClose: () => void;
  loadStatistics?: () => Promise<DocumentStatistics>;
  loadHistory?: () => Promise<SearchHistoryRecord[]>;
  deleteHistory?: (id: string) => Promise<boolean | void>;
  clearHistory?: () => Promise<number | void>;
  onApplyFilter?: (filter: StatisticsSearchFilter) => void;
  onSearchHistory?: (query: string) => void;
  registeredFolderIds?: string[];
}

function decimal(value: string) {
  try {
    return BigInt(value);
  } catch {
    return 0n;
  }
}

function formatInteger(value: string) {
  return decimal(value).toLocaleString("ko-KR");
}

function parseDateValue(value: string) {
  const trimmed = value.trim();
  if (/^\d+$/.test(trimmed)) {
    try {
      const raw = BigInt(trimmed);
      // File metadata can arrive as Unix seconds, milliseconds, or nanoseconds.
      const milliseconds =
        trimmed.length >= 16
          ? raw / 1_000_000n
          : trimmed.length >= 13
            ? raw
            : raw * 1_000n;
      const parsed = new Date(Number(milliseconds));
      if (Number.isFinite(parsed.getTime())) return parsed;
    } catch {
      // Fall through to the ISO/date parser below.
    }
  }
  const parsed = new Date(trimmed);
  return Number.isFinite(parsed.getTime()) ? parsed : null;
}

function formatModifiedDate(value: string) {
  const parsed = parseDateValue(value);
  return parsed
    ? parsed.toLocaleString("ko-KR", {
        dateStyle: "medium",
        timeStyle: "short",
      })
    : "날짜 없음";
}

function formatBytes(encodedBytes: string) {
  const bytes = decimal(encodedBytes);
  if (bytes < 1024n) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB", "PB", "EB"];
  let value = Number(bytes) / 1024;
  let unit = units[0];
  for (let index = 1; value >= 1024 && index < units.length; index += 1) {
    value /= 1024;
    unit = units[index];
  }
  return `${value.toFixed(value >= 10 ? 1 : 2)} ${unit}`;
}

function ratioPercent(value: string, total: bigint) {
  const count = decimal(value);
  return total === 0n ? 0 : Number((count * 10_000n) / total) / 100;
}

function donutGradient(buckets: StatisticsBucket[]) {
  const total = buckets.reduce((sum, bucket) => sum + decimal(bucket.count), 0n);
  if (total === 0n) return "conic-gradient(#e8dfd1 0 100%)";
  let cursor = 0;
  const segments = buckets.slice(0, 10).map((bucket, index) => {
    const start = cursor;
    cursor += ratioPercent(bucket.count, total);
    return `${CHART_COLORS[index % CHART_COLORS.length]} ${start}% ${cursor}%`;
  });
  if (cursor < 100) segments.push(`#c9bba7 ${cursor}% 100%`);
  return `conic-gradient(${segments.join(", ")})`;
}

function DistributionTable({
  buckets,
  label,
  onSelect,
}: {
  buckets: StatisticsBucket[];
  label: string;
  onSelect: (bucket: StatisticsBucket) => void;
}) {
  const total = buckets.reduce((sum, bucket) => sum + decimal(bucket.count), 0n);
  return (
    <div className="extension-distribution">
      <div className="donut-chart" style={{ background: donutGradient(buckets) }} aria-hidden="true">
        <div><strong>{total.toLocaleString("ko-KR")}</strong><span>총 문서</span></div>
      </div>
      <table className="data-table distribution-table" aria-label={label}>
        <thead className="sr-only"><tr><th scope="col">구분</th><th scope="col">문서 수</th><th scope="col">비율</th></tr></thead>
        <tbody>
          {buckets.slice(0, 10).map((bucket, index) => (
            <tr key={bucket.label}>
              <th scope="row"><button type="button" className="chart-segment" aria-label={`${bucket.label.toUpperCase()} 문서 ${bucket.count}개 검색`} onClick={() => onSelect(bucket)}>
                <span className="chart-swatch" style={{ background: CHART_COLORS[index % CHART_COLORS.length] }} aria-hidden="true" />{bucket.label.toUpperCase()}
              </button></th>
              <td>{formatInteger(bucket.count)}</td>
              <td className="sr-only">{Math.round(ratioPercent(bucket.count, total))}%</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function StatisticsDialog({
  open,
  onClose,
  loadStatistics = getStatistics,
  loadHistory = () => listSearchHistory(100, 0),
  deleteHistory = deleteSearchHistory,
  clearHistory,
  onApplyFilter,
  onSearchHistory,
  registeredFolderIds,
}: StatisticsDialogProps) {
  const [activeTab, setActiveTab] = useState<StatisticsTab>("documents");
  const [statistics, setStatistics] = useState<DocumentStatistics | null>(null);
  const [error, setError] = useState("");
  const close = useCallback(() => onClose(), [onClose]);
  const dialogRef = useModalDialog(open, close);

  useEffect(() => {
    if (!open) return;
    setError("");
    void loadStatistics().then(setStatistics).catch(() => {
      setError("통계를 불러오지 못했습니다.");
    });
  }, [loadStatistics, open]);

  if (!open) return null;

  return (
    <div className="modal-backdrop">
      <div
        ref={dialogRef}
        className="app-dialog statistics-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="statistics-title"
      >
        <header className="dialog-header">
          <h2 id="statistics-title">통계</h2>
          <button type="button" aria-label="통계 닫기" onClick={close}>×</button>
        </header>
        <div
          className="dialog-tabs"
          role="tablist"
          aria-label="통계 항목"
          onKeyDown={(event) => {
            if (!["ArrowRight", "ArrowLeft"].includes(event.key)) return;
            event.preventDefault();
            const next = activeTab === "documents" ? "history" : "documents";
            setActiveTab(next);
            window.requestAnimationFrame(() =>
              document.getElementById(`statistics-tab-${next}`)?.focus(),
            );
          }}
        >
          <button
            type="button"
            role="tab"
            id="statistics-tab-documents"
            aria-selected={activeTab === "documents"}
            aria-controls="statistics-panel"
            tabIndex={activeTab === "documents" ? 0 : -1}
            onClick={() => setActiveTab("documents")}
          >
            문서 통계
          </button>
          <button
            type="button"
            role="tab"
            id="statistics-tab-history"
            aria-selected={activeTab === "history"}
            aria-controls="statistics-panel"
            tabIndex={activeTab === "history" ? 0 : -1}
            onClick={() => setActiveTab("history")}
          >
            검색 히스토리
          </button>
        </div>
        <div
          id="statistics-panel"
          className="dialog-body statistics-body"
          role="tabpanel"
          aria-labelledby={`statistics-tab-${activeTab}`}
          tabIndex={0}
        >
          {activeTab === "history" ? (
            <>
              {statistics && (
                <section className="search-statistics-summary" aria-label="검색 통계 요약">
                  <div><strong>{formatInteger(statistics.totalSearches)}</strong><span>총 검색 횟수</span></div>
                  <div><strong>{formatInteger(statistics.uniqueSearchTerms)}</strong><span>고유 검색어</span></div>
                </section>
              )}
              <SearchHistoryTab
                loadHistory={loadHistory}
                deleteHistory={deleteHistory}
                clearHistory={clearHistory}
                onSearch={onSearchHistory}
                frequentSearches={statistics?.frequentSearches}
              />
            </>
          ) : !statistics ? (
            <p role="status">{error || "문서 통계를 계산하는 중…"}</p>
          ) : (
            <>
              <section className="statistics-summary" aria-label="문서 통계 요약">
                <div><strong>{formatInteger(statistics.totalDocuments)}</strong><span>총 문서</span></div>
                <div><strong>{formatInteger(statistics.indexedDocuments)}</strong><span>색인 완료</span></div>
                <div><strong>{formatBytes(statistics.totalBytes)}</strong><span>총 크기</span></div>
              </section>
              <div className="statistics-grid statistics-grid--stacked">
                <section>
                  <h3>파일 유형별 분포</h3>
                  <DistributionTable
                    buckets={statistics.byExtension}
                    label="파일 유형별 문서 수"
                    onSelect={(bucket) => {
                      onApplyFilter?.(
                        bucket.label === "(none)"
                          ? { extensionless: true }
                          : { extensions: [bucket.label.toLowerCase()] },
                      );
                      close();
                    }}
                  />
                </section>
                <section>
                  <h3>폴더별 문서 수</h3>
                  <table className="data-table" aria-label="폴더별 문서 수">
                    <thead><tr><th scope="col">폴더</th><th scope="col">문서 수</th></tr></thead>
                    <tbody>
                      {statistics.byFolder
                        .filter(
                          (bucket) =>
                            !registeredFolderIds ||
                            registeredFolderIds.includes(bucket.id),
                        )
                        .map((bucket) => (
                        <tr key={bucket.id}>
                          <th scope="row">
                            <button
                              type="button"
                              className="table-link"
                              onClick={() => {
                                onApplyFilter?.({ folderIds: [bucket.id] });
                                close();
                              }}
                            >
                              {bucket.label}
                            </button>
                          </th>
                          <td>
                            <span
                              className="statistics-bar"
                              style={{
                                width: `${Math.max(3, Math.round(ratioPercent(
                                  bucket.count,
                                  statistics.byFolder.reduce(
                                    (largest, item) => decimal(item.count) > largest ? decimal(item.count) : largest,
                                    1n,
                                  ),
                                )))}%`,
                              }}
                              aria-hidden="true"
                            />
                            {formatInteger(bucket.count)}
                          </td>
                        </tr>
                        ))}
                    </tbody>
                  </table>
                </section>
              </div>
              <div className="statistics-grid">
                <section>
                  <h3>연도별 문서 수</h3>
                  <table className="data-table" aria-label="연도별 문서 수">
                    <thead><tr><th scope="col">연도</th><th scope="col">문서 수</th></tr></thead>
                    <tbody>
                      {statistics.byYear.map((bucket) => (
                        <tr key={bucket.label}>
                          <th scope="row">{bucket.label}</th>
                          <td>
                            <span
                              className="statistics-bar"
                              style={{
                                width: `${Math.max(
                                  3,
                                  Math.round(
                                    ratioPercent(
                                      bucket.count,
                                      statistics.byYear.reduce(
                                        (largest, item) =>
                                          decimal(item.count) > largest
                                            ? decimal(item.count)
                                            : largest,
                                        1n,
                                      ),
                                    ),
                                  ),
                                )}%`,
                              }}
                              aria-hidden="true"
                            />
                            {formatInteger(bucket.count)}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </section>
                <section>
                  <h3>문서 처리 상태</h3>
                  <table className="data-table" aria-label="문서 처리 상태">
                    <thead><tr><th scope="col">상태</th><th scope="col">문서 수</th></tr></thead>
                    <tbody>
                      {statistics.parseStates.map((bucket) => (
                        <tr key={bucket.label}>
                          <th scope="row">{bucket.label}</th>
                          <td>{formatInteger(bucket.count)}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </section>
              </div>
              <div className="statistics-grid statistics-grid--rankings">
                <section>
                  <h3>최근 수정된 문서</h3>
                  <ol className="document-ranking">
                    {statistics.recentlyModified.map((document) => (
                      <li key={document.documentId}>
                        <span>{document.fileName}</span>
                        <time
                          dateTime={parseDateValue(document.modifiedAt)?.toISOString()}
                          title={formatModifiedDate(document.modifiedAt)}
                        >
                          {formatModifiedDate(document.modifiedAt)}
                        </time>
                      </li>
                    ))}
                  </ol>
                </section>
                <section>
                  <h3>가장 큰 문서</h3>
                  <ol className="document-ranking">
                    {statistics.largestDocuments.map((document) => (
                      <li key={document.documentId}>
                        <span>{document.fileName}</span>
                        <span>{formatBytes(document.sizeBytes)}</span>
                      </li>
                    ))}
                  </ol>
                </section>
              </div>
            </>
          )}
        </div>
        <footer className="dialog-footer">
          <span>통계와 검색 기록은 이 PC에만 저장됩니다.</span>
          <button type="button" onClick={close}>닫기</button>
        </footer>
      </div>
    </div>
  );
}
