import { useState } from "react";
import type { IndexStatus as IndexStatusModel } from "../../lib/types";

interface Props {
  status: IndexStatusModel;
  onPause: (jobId: string) => void;
  onResume: (jobId: string) => void;
  onCancel: (jobId: string) => void;
}

function fileName(path: string | null) {
  if (!path) return null;
  const segments = path.split(/[\\/]/);
  return segments[segments.length - 1] || path;
}

export function IndexStatus({
  status,
  onPause,
  onResume,
  onCancel,
}: Props) {
  const [showPath, setShowPath] = useState(false);
  const currentFileName = fileName(status.currentPath);
  const active = !["completed", "cancelled", "failed"].includes(status.state);

  return (
    <section aria-label="색인 진행 상태">
      <progress
        aria-label="색인 진행률"
        max={status.totalFiles || 1}
        value={status.completedFiles}
      />
      <p>
        {status.completedFiles.toLocaleString()} /{" "}
        {status.totalFiles.toLocaleString()}
      </p>
      {currentFileName && <p>{currentFileName}</p>}
      {status.currentPath && (
        <>
          <button
            type="button"
            aria-label="경로 세부정보 보기"
            aria-expanded={showPath}
            onClick={() => setShowPath((visible) => !visible)}
          >
            세부정보
          </button>
          {showPath && <code>{status.currentPath}</code>}
        </>
      )}
      <p>오류 {status.errors.length.toLocaleString()}개</p>
      {active && (
        <div>
          {status.state === "paused" ? (
            <button type="button" onClick={() => onResume(status.jobId)}>
              계속
            </button>
          ) : (
            <button type="button" onClick={() => onPause(status.jobId)}>
              일시정지
            </button>
          )}
          <button type="button" onClick={() => onCancel(status.jobId)}>
            취소
          </button>
        </div>
      )}
    </section>
  );
}
