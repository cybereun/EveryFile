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

export interface StatisticsDialogProps {
  open: boolean;
  onClose: () => void;
  loadStatistics?: () => Promise<DocumentStatistics>;
  loadHistory?: () => Promise<SearchHistoryRecord[]>;
  deleteHistory?: (id: string) => Promise<void>;
  clearHistory?: () => Promise<void>;
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
    <table className="data-table distribution-table" aria-label={label}>
      <thead>
        <tr>
          <th scope="col">구분</th>
          <th scope="col">문서 수</th>
          <th scope="col">비율</th>
        </tr>
      </thead>
      <tbody>
        {buckets.map((bucket) => (
          <tr key={bucket.label}>
            <th scope="row">
              <button
                type="button"
                className="chart-segment"
                aria-label={`${bucket.label.toUpperCase()} 문서 ${bucket.count}개 검색`}
                onClick={() => onSelect(bucket)}
              >
                <span className="chart-swatch" aria-hidden="true" />
                {bucket.label.toUpperCase()}
              </button>
            </th>
            <td>{formatInteger(bucket.count)}</td>
            <td>{Math.round(ratioPercent(bucket.count, total))}%</td>
          </tr>
        ))}
      </tbody>
    </table>
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
            <SearchHistoryTab
              loadHistory={loadHistory}
              deleteHistory={deleteHistory}
              clearHistory={clearHistory}
              onSearch={onSearchHistory}
              frequentSearches={statistics?.frequentSearches}
            />
          ) : !statistics ? (
            <p role="status">{error || "문서 통계를 계산하는 중…"}</p>
          ) : (
            <>
              <section className="statistics-summary" aria-label="문서 통계 요약">
                <div><strong>{formatInteger(statistics.totalDocuments)}</strong><span>총 문서</span></div>
                <div><strong>{formatInteger(statistics.indexedDocuments)}</strong><span>색인 완료</span></div>
                <div><strong>{formatBytes(statistics.totalBytes)}</strong><span>총 크기</span></div>
              </section>
              <div className="statistics-grid">
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
                          <td>{formatInteger(bucket.count)}</td>
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
              <div className="statistics-grid">
                <section>
                  <h3>최근 수정된 문서</h3>
                  <ol className="document-ranking">
                    {statistics.recentlyModified.map((document) => (
                      <li key={document.documentId}>
                        <span>{document.fileName}</span>
                        <time dateTime={document.modifiedAt}>
                          {new Date(document.modifiedAt).toLocaleDateString()}
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
              <section className="search-statistics-summary" aria-label="검색 통계 요약">
                <div><strong>{formatInteger(statistics.totalSearches)}</strong><span>총 검색 횟수</span></div>
                <div><strong>{formatInteger(statistics.uniqueSearchTerms)}</strong><span>고유 검색어</span></div>
              </section>
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
