import { useCallback, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useModalDialog } from "../../components/useModalDialog";
import type { AppSettings } from "../../lib/types";

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
  const deferredSetting = (
    key: "minimizeToTray" | "startWithWindows" | "startHidden",
  ) => (
    <label className="settings-check">
      <input
        type="checkbox"
        checked={false}
        disabled
        aria-describedby={`${key}-deferred`}
        readOnly
      />
      <span>
        {{
          minimizeToTray: "닫을 때 알림 영역으로 최소화",
          startWithWindows: "Windows 시작 시 실행",
          startHidden: "숨김 상태로 시작",
        }[key]}
        <small id={`${key}-deferred`}>향후 버전에서 제공됩니다.</small>
      </span>
    </label>
  );

  return (
    <div className="settings-grid">
      {deferredSetting("startWithWindows")}
      {deferredSetting("startHidden")}
      {deferredSetting("minimizeToTray")}
      <label>
        색인 강도
        <select
          aria-label="색인 강도"
          value={settings.indexingIntensity ?? "balanced"}
          onChange={(event) =>
            onChange({
              ...settings,
              indexingIntensity: event.target.value as "low" | "balanced" | "high",
            })
          }
        >
          <option value="low">조용히</option>
          <option value="balanced">균형</option>
          <option value="high">빠르게</option>
        </select>
      </label>
      <section className="danger-zone" aria-labelledby="reset-data-heading">
        <h3 id="reset-data-heading">로컬 데이터 초기화</h3>
        <p>색인, 북마크, 태그, 검색 히스토리를 이 PC에서만 삭제합니다.</p>
        <button
          ref={resetTrigger}
          type="button"
          className="danger-button"
          disabled={!onReset}
          onClick={() => setConfirmation(true)}
        >
          모든 로컬 데이터 초기화
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
              <h3 id="reset-confirm-title">데이터 초기화 확인</h3>
              <p id="reset-confirm-description">
                이 작업은 되돌릴 수 없습니다. EveryFile을 빈 색인으로 다시 시작합니다.
              </p>
              {resetError && <p role="alert">{resetError}</p>}
              <div className="dialog-actions">
                <button type="button" onClick={() => setConfirmation(false)}>
                  취소
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
                          : "데이터 초기화를 시작하지 못했습니다.",
                      );
                      setResetting(false);
                    }
                  }}
                >
                  초기화하고 다시 시작
                </button>
              </div>
            </section>
          </div>,
          document.body,
        )}
    </div>
  );
}
