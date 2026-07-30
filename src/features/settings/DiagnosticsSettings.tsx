import type { ParseErrorRecord } from "../../lib/types";

interface DiagnosticsSettingsProps {
  errors: ParseErrorRecord[];
  logFolder?: string;
  onRetry?: (documentId: string) => Promise<void>;
}

export function DiagnosticsSettings({
  errors,
  logFolder,
  onRetry,
}: DiagnosticsSettingsProps) {
  return (
    <div className="settings-grid">
      <section className="settings-card">
        <h3>진단 로그</h3>
        <p>로그는 이 PC의 앱 데이터 폴더에만 7일간 보관되며 자동 전송되지 않습니다.</p>
        <code>{logFolder ?? "EveryFile 앱 데이터 / logs"}</code>
      </section>
      <section className="settings-card" aria-labelledby="parse-errors-heading">
        <h3 id="parse-errors-heading">문서 처리 오류</h3>
        {errors.length === 0 ? (
          <p>현재 문서 처리 오류가 없습니다.</p>
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
                  다시 시도
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
