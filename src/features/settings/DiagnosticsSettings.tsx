import type { AvailableUpdate } from "../../lib/updater";
import type { AppSettings, ParseErrorRecord } from "../../lib/types";
import { useI18n } from "../../app/translations";

interface DiagnosticsSettingsProps {
  settings: AppSettings;
  errors: ParseErrorRecord[];
  logFolder?: string;
  onRetry?: (documentId: string) => Promise<void>;
  onChange: (settings: AppSettings) => void;
  onCheckForUpdates?: () => Promise<AvailableUpdate | null>;
  updateChecking?: boolean;
  updateStatus?: string;
}

export function DiagnosticsSettings({
  settings,
  errors,
  logFolder,
  onRetry,
  onChange,
  onCheckForUpdates,
  updateChecking = false,
  updateStatus = "",
}: DiagnosticsSettingsProps) {
  const { t } = useI18n();
  return (
    <div className="settings-grid">
      <section className="settings-card update-settings-card" aria-labelledby="update-heading">
        <div className="settings-card__heading-row">
          <div>
        <h3 id="update-heading">{t("업데이트")}</h3>
            <p>{t("새 버전이 있으면 서명된 설치 파일을 확인하고 알립니다. 문서와 색인 데이터는 전송되지 않습니다.")}</p>
          </div>
          <button
            type="button"
            className="settings-inline-button"
            disabled={!onCheckForUpdates || updateChecking}
            onClick={() => void onCheckForUpdates?.()}
          >
            {updateChecking ? t("확인 중…") : t("지금 확인")}
          </button>
        </div>
        <label className="settings-check update-toggle">
          <input
            type="checkbox"
            checked={settings.autoUpdateEnabled ?? true}
            onChange={(event) =>
              onChange({ ...settings, autoUpdateEnabled: event.target.checked })
            }
          />
          <span>
            <strong>{t("자동 업데이트 확인")}</strong>
            <small>{t("앱 시작 시와 6시간마다 확인 · 새 버전 발견 시 알림")}</small>
          </span>
        </label>
        {updateStatus && <p className="settings-inline-status" role="status">{updateStatus}</p>}
      </section>
      <section className="settings-card">
        <h3>{t("진단 로그")}</h3>
        <p>{t("로그는 이 PC의 앱 데이터 폴더에만 7일간 보관되며 자동 전송되지 않습니다.")}</p>
        <code>{logFolder ?? t("EveryFile 앱 데이터 / logs")}</code>
      </section>
      <section className="settings-card" aria-labelledby="parse-errors-heading">
        <h3 id="parse-errors-heading">{t("문서 처리 오류")}</h3>
        {errors.length === 0 ? (
          <p>{t("현재 문서 처리 오류가 없습니다.")}</p>
        ) : (
          <ul className="diagnostic-list">
            {errors.map((error) => (
              <li key={error.documentId}>
                <span>
                  <strong>{error.fileName}</strong>
                  <small>{error.errorCode}</small>
                </span>
                <button
                  type="button"
                  disabled={!onRetry}
                  onClick={() => void onRetry?.(error.documentId)}
                >
                  {t("다시 시도")}
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
