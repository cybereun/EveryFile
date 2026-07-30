import type { AppSettings } from "../../lib/types";

interface GeneralSettingsProps {
  settings: AppSettings;
  onChange: (settings: AppSettings) => void;
}

export function GeneralSettings({ settings, onChange }: GeneralSettingsProps) {
  const update = <Key extends keyof AppSettings>(key: Key, value: AppSettings[Key]) =>
    onChange({ ...settings, [key]: value });

  return (
    <div className="settings-grid">
      <label>
        언어
        <select
          value={settings.language}
          onChange={(event) => update("language", event.target.value)}
        >
          <option value="ko">한국어</option>
          <option value="en">English</option>
        </select>
      </label>
      <label>
        결과 밀도
        <select
          value={settings.resultPageSize}
          onChange={(event) => update("resultPageSize", Number(event.target.value))}
        >
          <option value={50}>넓게 · 50개</option>
          <option value={100}>보통 · 100개</option>
          <option value={200}>조밀하게 · 200개</option>
        </select>
      </label>
      <label>
        파일 클릭 동작
        <select
          aria-label="파일 클릭 동작"
          value={settings.fileClickBehavior ?? "preview"}
          onChange={(event) =>
            update("fileClickBehavior", event.target.value as "preview" | "open")
          }
        >
          <option value="preview">미리보기</option>
          <option value="open">원본 파일 열기</option>
        </select>
      </label>
      <label>
        날짜 표시
        <select
          aria-label="날짜 표시"
          value={settings.dateDisplay ?? "relative"}
          onChange={(event) =>
            update("dateDisplay", event.target.value as "relative" | "absolute")
          }
        >
          <option value="relative">상대 날짜</option>
          <option value="absolute">전체 날짜</option>
        </select>
      </label>
    </div>
  );
}
