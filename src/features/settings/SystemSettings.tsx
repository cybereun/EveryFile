import { useState } from "react";
import type { AppSettings } from "../../lib/types";

interface SystemSettingsProps {
  settings: AppSettings;
  onChange: (settings: AppSettings) => void;
  onReset?: () => Promise<void>;
}

export function SystemSettings({ settings, onChange, onReset }: SystemSettingsProps) {
  const [confirming, setConfirming] = useState(false);
  const [resetting, setResetting] = useState(false);
  const toggle = (key: "minimizeToTray" | "startWithWindows" | "startHidden") => (
    <label className="settings-check">
      <input
        type="checkbox"
        checked={settings[key]}
        onChange={(event) => onChange({ ...settings, [key]: event.target.checked })}
      />
      {{
        minimizeToTray: "닫을 때 알림 영역으로 최소화",
        startWithWindows: "Windows 시작 시 실행",
        startHidden: "숨김 상태로 시작",
      }[key]}
    </label>
  );

  return (
    <div className="settings-grid">
      {toggle("startWithWindows")}
      {toggle("startHidden")}
      {toggle("minimizeToTray")}
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
          type="button"
          className="danger-button"
          disabled={!onReset}
          onClick={() => setConfirming(true)}
        >
          모든 로컬 데이터 초기화
        </button>
      </section>
      {confirming && (
        <div className="confirmation-backdrop">
          <section
            className="confirmation-dialog"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby="reset-confirm-title"
          >
            <h3 id="reset-confirm-title">데이터 초기화 확인</h3>
            <p>이 작업은 되돌릴 수 없습니다. EveryFile을 빈 색인으로 다시 시작합니다.</p>
            <div className="dialog-actions">
              <button type="button" onClick={() => setConfirming(false)}>
                취소
              </button>
              <button
                type="button"
                className="danger-button"
                disabled={resetting}
                onClick={async () => {
                  if (!onReset) return;
                  setResetting(true);
                  await onReset();
                }}
              >
                초기화하고 다시 시작
              </button>
            </div>
          </section>
        </div>
      )}
    </div>
  );
}
