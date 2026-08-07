import { listen } from "@tauri-apps/api/event";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { cancelIndexing, pauseIndexing, resumeIndexing } from "../../lib/ipc";
import type { IndexStatus as IndexStatusModel } from "../../lib/types";

interface Props {
  status: IndexStatusModel;
  onPause: (jobId: string) => void;
  onResume: (jobId: string) => void;
  onCancel: (jobId: string) => void;
}

function fileName(path: string | null) {
  if (!path) return "준비 중";
  const segments = path.split(/[\\/]/);
  return segments[segments.length - 1] || path;
}

const phaseLabels: Record<IndexStatusModel["state"], string> = {
  queued: "대기 중",
  discovering: "파일 확인 중",
  parsing: "색인 중",
  paused: "일시정지",
  completed: "완료",
  cancelled: "취소됨",
  failed: "중단됨",
};

export function IndexStatus({ status, onPause, onResume, onCancel }: Props) {
  const total = Math.max(0, status.totalFiles);
  const completed = Math.min(Math.max(0, status.completedFiles), total || 1);
  const percentage = total > 0 ? Math.min(100, Math.round((completed / total) * 100)) : 0;
  const errorCount = status.errorCount ?? status.errors.length;

  return (
    <div className="index-progress" role="status" aria-label="색인 진행 상태" aria-live="polite">
      <div className="index-progress__line">
        <span className="index-progress__phase">
          <span className="index-progress__dot" aria-hidden="true" />
          {phaseLabels[status.state]}
        </span>
        <span className="index-progress__count">
          {status.completedFiles.toLocaleString()} / {status.totalFiles.toLocaleString()}
        </span>
        <span className="index-progress__file" title={status.currentPath ?? undefined}>
          {fileName(status.currentPath)}
        </span>
        {errorCount > 0 && (
          <span className="index-progress__errors">실패 {errorCount.toLocaleString()}건</span>
        )}
        <strong className="index-progress__percent">{percentage}%</strong>
        {status.state === "paused" ? (
          <button type="button" onClick={() => onResume(status.jobId)}>계속</button>
        ) : (
          <button type="button" onClick={() => onPause(status.jobId)}>일시정지</button>
        )}
        <button type="button" onClick={() => onCancel(status.jobId)}>취소</button>
      </div>
      <progress
        className="index-progress__bar"
        aria-label="색인 진행률"
        max={total || 1}
        value={completed}
      />
    </div>
  );
}

function IndexingReport({ status, onClose }: { status: IndexStatusModel; onClose: () => void }) {
  const failures = status.errorCount;
  const successes = Math.max(0, status.completedFiles - failures);

  useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", closeOnEscape);
    return () => document.removeEventListener("keydown", closeOnEscape);
  }, [onClose]);

  return (
    <div
      className="modal-backdrop index-report-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <section className="index-report" role="dialog" aria-modal="true" aria-labelledby="index-report-title">
        <header>
          <h2 id="index-report-title">색인 결과</h2>
          <button type="button" className="icon-button" aria-label="색인 결과 닫기" onClick={onClose}>×</button>
        </header>
        <div className="index-report__totals">
          <div><strong className="is-success">{successes.toLocaleString()}</strong><span>성공</span></div>
          <div><strong className="is-failure">{failures.toLocaleString()}</strong><span>실패</span></div>
        </div>
        {failures > 0 && (
          <details className="index-report__errors">
            <summary>오류 ({failures.toLocaleString()}건)</summary>
            <div className="index-report__error-list">
              {status.errors.map((error, index) => (
                <article key={`${error.code}-${error.fileName}-${index}`}>
                  <strong>{error.fileName || "알 수 없는 파일"}</strong>
                  <span>{error.code}: {error.message}</span>
                </article>
              ))}
              {failures > status.errors.length && (
                <p>나머지 {(failures - status.errors.length).toLocaleString()}건은 진단 메뉴에서 확인할 수 있습니다.</p>
              )}
            </div>
          </details>
        )}
        <footer><button type="button" onClick={onClose}>닫기</button></footer>
      </section>
    </div>
  );
}

interface IndexStatusControllerProps {
  idleContent?: ReactNode;
  /** Kept for API compatibility; watcher jobs are filtered by status.silent. */
  reportJobIds?: ReadonlySet<string>;
}

export function IndexStatusController({
  idleContent = null,
}: IndexStatusControllerProps) {
  const [status, setStatus] = useState<IndexStatusModel | null>(null);
  const [report, setReport] = useState<IndexStatusModel | null>(null);
  const reportedJobs = useRef(new Set<string>());
  const activeJobId = useRef<string | null>(null);

  useEffect(() => {
    let disposed = false;
    let removeListener: (() => void) | undefined;
    void listen<IndexStatusModel>("index-status://changed", (event) => {
      const payload = {
        ...event.payload,
        errorCount: event.payload.errorCount ?? event.payload.errors.length,
      };
      if (payload.silent) return;
      if (["completed", "failed"].includes(payload.state)) {
        // A worker can publish one last progress snapshot after its terminal
        // event has already reached the webview. Ignore that stale snapshot so
        // the progress bar cannot reappear behind the completion report.
        if (activeJobId.current && activeJobId.current !== payload.jobId) return;
        setStatus(null);
        activeJobId.current = null;
        if (reportedJobs.current.has(payload.jobId)) return;
        reportedJobs.current.add(payload.jobId);
        setReport(payload);
      } else if (payload.state === "cancelled") {
        if (activeJobId.current && activeJobId.current !== payload.jobId) return;
        activeJobId.current = null;
        setStatus(null);
      } else {
        if (reportedJobs.current.has(payload.jobId)) return;
        activeJobId.current = payload.jobId;
        setStatus(payload);
      }
    })
      .then((unlisten) => {
        if (disposed) unlisten();
        else removeListener = unlisten;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      removeListener?.();
    };
  }, []);

  return (
    <>
      {report ? null : status ? (
        <IndexStatus
          status={status}
          onPause={(jobId) => void pauseIndexing(jobId)}
          onResume={(jobId) => void resumeIndexing(jobId)}
          onCancel={(jobId) => void cancelIndexing(jobId)}
        />
      ) : idleContent}
      {report && (
        <IndexingReport
          status={report}
          onClose={() => setReport(null)}
        />
      )}
    </>
  );
}
