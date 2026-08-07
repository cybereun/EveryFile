import { useState } from "react";
import type { AvailableUpdate, UpdateProgress } from "../../lib/updater";

interface UpdateDialogProps {
  update: AvailableUpdate | null;
  onClose: () => void;
}

function formatDate(value?: string) {
  if (!value) return "";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "" : date.toLocaleDateString();
}

function progressLabel(progress: UpdateProgress | null) {
  if (!progress) return "다운로드 후 앱이 자동으로 다시 시작됩니다.";
  if (progress.phase === "finished") return "설치 준비가 완료되었습니다. 앱을 다시 시작합니다…";
  if (progress.phase === "starting") return "업데이트 파일을 준비하는 중…";
  if (progress.percent !== undefined) return `다운로드 중 ${progress.percent}%`;
  return "업데이트 파일을 다운로드하는 중…";
}

export function UpdateDialog({ update, onClose }: UpdateDialogProps) {
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<UpdateProgress | null>(null);
  const [error, setError] = useState("");

  if (!update) return null;

  const install = async () => {
    setInstalling(true);
    setError("");
    try {
      await update.install(setProgress);
    } catch {
      setInstalling(false);
      setError("업데이트를 설치하지 못했습니다. 네트워크와 권한을 확인한 뒤 다시 시도하세요.");
    }
  };

  return (
    <div className="modal-backdrop update-modal-backdrop">
      <section
        className="app-dialog update-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="update-dialog-title"
      >
        <header className="dialog-header">
          <h2 id="update-dialog-title">업데이트 {update.version} 사용 가능</h2>
          <button type="button" aria-label="업데이트 창 닫기" onClick={onClose} disabled={installing}>×</button>
        </header>
        <div className="update-dialog__body">
          <p className="update-dialog__lead">새 버전 <strong>{update.version}</strong>이(가) 배포되었습니다.</p>
          {formatDate(update.date) && <p className="update-dialog__date">게시일 {formatDate(update.date)}</p>}
          <pre className="update-dialog__notes">{update.notes}</pre>
          {error && <p className="update-dialog__error" role="alert">{error}</p>}
          {installing && (
            <div className="update-dialog__progress" role="status" aria-live="polite">
              <span>{progressLabel(progress)}</span>
              <progress value={progress?.percent} max={100} />
            </div>
          )}
        </div>
        <footer className="dialog-footer">
          <span>다운로드 후 앱이 자동 재시작됩니다. 설정과 색인은 유지됩니다.</span>
          <button type="button" onClick={onClose} disabled={installing}>나중에</button>
          <button type="button" className="primary-button" onClick={() => void install()} disabled={installing}>
            {installing ? "설치 중…" : "지금 설치"}
          </button>
        </footer>
      </section>
    </div>
  );
}
