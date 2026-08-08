import { useCallback, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useModalDialog } from "../../components/useModalDialog";
import type { AppSettings } from "../../lib/types";
import { useI18n } from "../../app/translations";

interface SystemSettingsProps {
  settings: AppSettings;
  onChange: (settings: AppSettings) => void;
  onReset?: () => Promise<void>;
  onConfirmationChange?: (open: boolean) => void;
}

export function SystemSettings({
  settings,
  onChange,
  onReset,
  onConfirmationChange,
}: SystemSettingsProps) {
  const { t } = useI18n();
  const [confirming, setConfirming] = useState(false);
  const [resetting, setResetting] = useState(false);
  const [resetError, setResetError] = useState("");
  const resetTrigger = useRef<HTMLButtonElement>(null);
  const setConfirmation = useCallback(
    (open: boolean) => {
      setConfirming(open);
      onConfirmationChange?.(open);
      if (!open) {
        setResetting(false);
        setResetError("");
        window.requestAnimationFrame(() => resetTrigger.current?.focus());
      }
    },
    [onConfirmationChange],
  );
  const confirmationRef = useModalDialog(confirming, () => setConfirmation(false));
  const systemSetting = (
    key: "minimizeToTray" | "startWithWindows" | "startHidden",
  ) => (
    <label className="settings-check">
      <input
        type="checkbox"
        checked={settings[key]}
        aria-label={{
          startWithWindows: t("Windows 시작 시 실행"),
          startHidden: t("숨김 상태로 시작"),
          minimizeToTray: t("닫을 때 알림 영역으로 최소화"),
        }[key]}
        aria-describedby={`${key}-description`}
        onChange={(event) => onChange({ ...settings, [key]: event.target.checked })}
      />
      <span>
        {{
          startWithWindows: t("Windows 시작 시 실행"),
          startHidden: t("숨김 상태로 시작"),
          minimizeToTray: t("닫을 때 알림 영역으로 최소화"),
        }[key]}
        <small id={`${key}-description`}>
          {{
            startWithWindows: t("Windows에 로그인하면 EveryFile을 자동으로 실행합니다."),
            startHidden: t("시작할 때 창을 숨기고 알림 영역에서 대기합니다."),
            minimizeToTray: t("닫기 버튼을 눌러도 앱을 종료하지 않고 알림 영역으로 보냅니다."),
          }[key]}
        </small>
      </span>
    </label>
  );

  return (
    <div className="settings-grid">
      {systemSetting("startWithWindows")}
      {systemSetting("startHidden")}
      {systemSetting("minimizeToTray")}
      <label>
        {t("색인 강도")}
        <select
          aria-label={t("색인 강도")}
          value={settings.indexingIntensity ?? "balanced"}
          onChange={(event) =>
            onChange({
              ...settings,
              indexingIntensity: event.target.value as "low" | "balanced" | "high",
            })
          }
        >
          <option value="low">{t("조용히")}</option>
          <option value="balanced">{t("균형")}</option>
          <option value="high">{t("빠르게")}</option>
        </select>
      </label>
      <section className="danger-zone" aria-labelledby="reset-data-heading">
        <h3 id="reset-data-heading">{t("로컬 데이터 초기화")}</h3>
        <p>{t("색인, 북마크, 태그, 검색 히스토리를 이 PC에서만 삭제합니다.")}</p>
        <button
          ref={resetTrigger}
          type="button"
          className="danger-button"
          disabled={!onReset}
          onClick={() => setConfirmation(true)}
        >
          {t("모든 로컬 데이터 초기화")}
        </button>
      </section>
      {confirming &&
        createPortal(
          <div className="confirmation-backdrop" data-modal-layer="">
            <section
              ref={confirmationRef}
              className="confirmation-dialog"
              role="alertdialog"
              aria-modal="true"
              aria-labelledby="reset-confirm-title"
              aria-describedby="reset-confirm-description"
            >
              <h3 id="reset-confirm-title">{t("데이터 초기화 확인")}</h3>
              <p id="reset-confirm-description">
                {t("이 작업은 되돌릴 수 없습니다. EveryFile을 빈 색인으로 다시 시작합니다.")}
              </p>
              {resetError && <p role="alert">{resetError}</p>}
              <div className="dialog-actions">
                <button type="button" onClick={() => setConfirmation(false)}>
                  {t("취소")}
                </button>
                <button
                  type="button"
                  className="danger-button"
                  disabled={resetting}
                  onClick={async () => {
                    if (!onReset) return;
                    setResetting(true);
                    setResetError("");
                    try {
                      await onReset();
                    } catch (error) {
                      setResetError(
                        error instanceof Error
                          ? error.message
                          : t("데이터 초기화를 시작하지 못했습니다."),
                      );
                      setResetting(false);
                    }
                  }}
                >
                  {t("초기화하고 다시 시작")}
                </button>
              </div>
            </section>
          </div>,
          document.body,
        )}
    </div>
  );
}
