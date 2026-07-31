import type { AppSettings, FolderRecord } from "../../lib/types";

interface SearchSettingsProps {
  settings: AppSettings;
  folders: FolderRecord[];
  onChange: (settings: AppSettings) => void;
}

export function SearchSettings({
  settings,
  folders,
  onChange,
}: SearchSettingsProps) {
  const ocrEnabled = settings.ocrEnabled ?? false;
  const mathOcrEnabled = settings.mathOcrEnabled ?? false;

  return (
    <div className="settings-grid">
      <section className="settings-card" aria-labelledby="ocr-heading">
        <div className="settings-toggle-row">
          <div>
            <h3 id="ocr-heading">로컬 OCR</h3>
            <p>
              스캔 PDF와 이미지의 글자를 PC 안에서 인식합니다. 문서와 이미지는
              외부 서버로 전송되지 않습니다.
            </p>
          </div>
          <input
            aria-label="로컬 OCR 활성화"
            type="checkbox"
            checked={ocrEnabled}
            onChange={(event) =>
              onChange({
                ...settings,
                ocrEnabled: event.target.checked,
                mathOcrEnabled: event.target.checked ? mathOcrEnabled : false,
              })
            }
          />
        </div>
        <p className="settings-help">
          지원 이미지: JPG, PNG, WebP, BMP, TIFF. 일반 PDF에 정상 텍스트가
          있으면 기존 텍스트를 사용하고 OCR을 건너뜁니다.
        </p>
        <div className="settings-toggle-row">
          <div>
            <strong>수학 OCR</strong>
            <p>
              수식이 포함된 PDF를 위한 별도 모델입니다. 모델이 크며 CPU 사용량과
              처리 시간이 크게 늘어납니다.
            </p>
          </div>
          <input
            aria-label="수학 OCR 활성화"
            type="checkbox"
            checked={mathOcrEnabled}
            disabled={!ocrEnabled}
            onChange={(event) =>
              onChange({ ...settings, mathOcrEnabled: event.target.checked })
            }
          />
        </div>
      </section>

      <label>
        검색 히스토리 보관 기간
        <select
          value={settings.historyRetentionDays}
          onChange={(event) =>
            onChange({
              ...settings,
              historyRetentionDays: Number(event.target.value),
            })
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
              maxFileSizeBytes:
                Math.max(1, Number(event.target.value)) * 1_048_576,
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
    </div>
  );
}
