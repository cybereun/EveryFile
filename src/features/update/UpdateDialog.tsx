import { useRef, useState } from "react";
import type { AvailableUpdate, UpdateProgress } from "../../lib/updater";
import { useI18n } from "../../app/translations";

interface UpdateDialogProps {
  update: AvailableUpdate | null;
  onClose: () => void;
}

function formatDate(value?: string) {
  if (!value) return "";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "" : date.toLocaleDateString();
}

function progressLabel(progress: UpdateProgress | null, t: (source: string) => string) {
  if (!progress) return t("다운로드 후 앱이 자동으로 다시 시작됩니다.");
  if (progress.phase === "verifying") return t("업데이트 서명을 확인하는 중…");
  if (progress.phase === "installing") return t("앱을 종료하고 설치 상태창으로 전환합니다…");
  if (progress.phase === "restarting") return t("설치가 완료되었습니다. 앱을 다시 시작합니다…");
  if (progress.phase === "starting") return t("업데이트 파일을 준비하는 중…");
  if (progress.percent !== undefined) return `${t("다운로드 중")} ${progress.percent}%`;
  return t("업데이트 파일을 다운로드하는 중…");
}

export function UpdateDialog({ update, onClose }: UpdateDialogProps) {
  const { t } = useI18n();
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<UpdateProgress | null>(null);
  const [error, setError] = useState("");
  const inFlight = useRef(false);

  if (!update) return null;

  const install = async () => {
    if (inFlight.current) return;
    inFlight.current = true;
    setInstalling(true);
    setProgress(null);
    setError("");
    try {
      await update.install(setProgress);
    } catch {
      setInstalling(false);
      setError(t("업데이트를 설치하지 못했습니다. 네트워크와 권한을 확인한 뒤 다시 시도하세요."));
    } finally {
      inFlight.current = false;
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
          <h2 id="update-dialog-title">{t("업데이트")} {update.version} {t("사용 가능")}</h2>
          <button type="button" aria-label={t("업데이트 창 닫기")} onClick={onClose} disabled={installing}>×</button>
        </header>
        <div className="update-dialog__body">
          <p className="update-dialog__lead">{t("새 버전")} <strong>{update.version}</strong>{t("이(가) 배포되었습니다.")}</p>
          {formatDate(update.date) && <p className="update-dialog__date">{t("게시일")} {formatDate(update.date)}</p>}
          <pre className="update-dialog__notes">{update.notes}</pre>
          <ol className="update-dialog__steps" aria-label={t("업데이트 순서")}>
            <li aria-current={!progress || ["starting", "downloading", "verifying"].includes(progress.phase) ? "step" : undefined}>{t("1. 다운로드 및 확인")}</li>
            <li aria-current={progress?.phase === "installing" ? "step" : undefined}>{t("2. 앱 종료 및 설치")}</li>
            <li aria-current={progress?.phase === "restarting" ? "step" : undefined}>{t("3. 자동 재실행")}</li>
          </ol>
          {error && <p className="update-dialog__error" role="alert">{error}</p>}
          {installing && (
            <div className="update-dialog__progress" role="status" aria-live="polite">
              <span>{progressLabel(progress, t)}</span>
              <progress value={progress?.percent} max={100} />
            </div>
          )}
        </div>
        <footer className="dialog-footer">
          <span>{t("다운로드 확인 후 앱이 종료되고 설치 상태창이 표시됩니다. 완료 후 자동 재실행되며 설정과 색인은 유지됩니다.")}</span>
          <button type="button" onClick={onClose} disabled={installing}>{t("나중에")}</button>
          <button type="button" className="primary-button" onClick={() => void install()} disabled={installing}>
            {installing ? t("설치 중…") : t("지금 설치")}
          </button>
        </footer>
      </section>
    </div>
  );
}
