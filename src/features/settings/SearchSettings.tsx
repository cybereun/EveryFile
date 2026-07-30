import type { AppSettings, FolderRecord } from "../../lib/types";

interface SearchSettingsProps {
  settings: AppSettings;
  folders: FolderRecord[];
  onChange: (settings: AppSettings) => void;
}

export function SearchSettings({ settings, folders, onChange }: SearchSettingsProps) {
  return (
    <div className="settings-grid">
      <label>
        검색 히스토리 보관 기간
        <select
          value={settings.historyRetentionDays}
          onChange={(event) =>
            onChange({ ...settings, historyRetentionDays: Number(event.target.value) })
          }
        >
          <option value={30}>30일</option>
          <option value={90}>90일</option>
          <option value={365}>365일</option>
          <option value={0}>제한 없음</option>
        </select>
      </label>
      <label>
        최대 파일 크기 (MB)
        <input
          min={1}
          max={4096}
          type="number"
          value={Math.round(settings.maxFileSizeBytes / 1_048_576)}
          onChange={(event) =>
            onChange({
              ...settings,
              maxFileSizeBytes: Math.max(1, Number(event.target.value)) * 1_048_576,
            })
          }
        />
      </label>
      <section className="settings-card" aria-labelledby="included-folders-heading">
        <h3 id="included-folders-heading">포함된 폴더</h3>
        {folders.length === 0 ? (
          <p>등록된 폴더가 없습니다.</p>
        ) : (
          <ul>
            {folders.map((folder) => (
              <li key={folder.id}>{folder.displayName}</li>
            ))}
          </ul>
        )}
      </section>
      <label>
        제외할 경로 패턴
        <textarea
          aria-describedby="exclude-path-help"
          placeholder="예: **/node_modules/**"
          rows={3}
          value={(settings.excludedPathPatterns ?? []).join("\n")}
          onChange={(event) =>
            onChange({
              ...settings,
              excludedPathPatterns: event.target.value
                .split(/\r?\n/)
                .map((value) => value.trim())
                .filter(Boolean),
            })
          }
        />
        <small id="exclude-path-help">한 줄에 하나씩 입력합니다.</small>
      </label>
      <div className="settings-disabled-row" aria-disabled="true">
        <div>
          <strong>파일 버전 그룹화</strong>
          <p>Phase 2에서 제공됩니다.</p>
        </div>
        <input aria-label="파일 버전 그룹화" type="checkbox" disabled />
      </div>
    </div>
  );
}
