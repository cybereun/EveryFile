import type { AppSettings } from "../../lib/types";
import { useI18n } from "../../app/translations";

interface AiSettingsProps {
  settings: AppSettings;
  hasSavedSecret: boolean;
  secretDraft: string | null;
  onChange: (settings: AppSettings) => void;
  onSecretDraftChange: (secret: string | null) => void;
}

const providerDefaults = {
  ollama: { baseUrl: "http://127.0.0.1:11434", model: "gemma3:4b" },
  gemini: {
    baseUrl: "https://generativelanguage.googleapis.com",
    model: "gemini-2.5-flash",
  },
  openai: { baseUrl: "https://api.openai.com", model: "gpt-4.1-mini" },
} as const;

export function AiSettings({
  settings,
  hasSavedSecret,
  secretDraft,
  onChange,
  onSecretDraftChange,
}: AiSettingsProps) {
  const { t } = useI18n();
  const enabled = settings.aiEnabled ?? false;
  const provider = settings.aiProvider ?? "ollama";
  const remote = provider !== "ollama";

  return (
    <div className="settings-grid">
      <section className="settings-card">
        <div className="settings-toggle-row">
          <div>
            <h3>{t("AI 기능 활성화")}</h3>
            <p>{t("문서 요약과 선택한 파일에 대한 질문 기능을 사용합니다.")}</p>
          </div>
          <input
            aria-label={t("AI 기능 활성화")}
            type="checkbox"
            checked={enabled}
            onChange={(event) =>
              onChange({ ...settings, aiEnabled: event.target.checked })
            }
          />
        </div>
      </section>

      {enabled && (
        <>
          <label>
            {t("LLM Provider")}
            <select
              aria-label={t("LLM Provider")}
              value={provider}
              onChange={(event) => {
                const next = event.target.value as keyof typeof providerDefaults;
                onSecretDraftChange(null);
                onChange({
                  ...settings,
                  aiProvider: next,
                  aiBaseUrl: providerDefaults[next].baseUrl,
                  aiModel: providerDefaults[next].model,
                });
              }}
            >
              <option value="ollama">Ollama ({t("로컬")})</option>
              <option value="gemini">Google Gemini API</option>
              <option value="openai">OpenAI API</option>
            </select>
          </label>
          <label>
            {t("Base URL")}
            <input
              value={settings.aiBaseUrl ?? providerDefaults[provider].baseUrl}
              onChange={(event) =>
                onChange({ ...settings, aiBaseUrl: event.target.value })
              }
            />
          </label>
          {remote && (
            <label>
            {t("API 키")}
              <input
                aria-label={`${t("AI")} ${t("API 키")}`}
                type="password"
                autoComplete="off"
                placeholder={
                  hasSavedSecret && secretDraft === null
                    ? t("저장된 키가 있습니다")
                    : t("API 키 입력")
                }
                value={secretDraft ?? ""}
                onChange={(event) => onSecretDraftChange(event.target.value)}
              />
              {hasSavedSecret && (
                <button type="button" onClick={() => onSecretDraftChange("")}>
                  {t("저장된 키 삭제")}
                </button>
              )}
            </label>
          )}
          <label>
            {t("AI 모델")}
            <input
              value={settings.aiModel ?? providerDefaults[provider].model}
              onChange={(event) =>
                onChange({ ...settings, aiModel: event.target.value })
              }
            />
          </label>
          <div className="settings-two-columns">
            <label>
              {t("온도")} ({(settings.aiTemperature ?? 0.2).toFixed(1)})
              <input
                min={0}
                max={2}
                step={0.1}
                type="range"
                value={settings.aiTemperature ?? 0.2}
                onChange={(event) =>
                  onChange({
                    ...settings,
                    aiTemperature: Number(event.target.value),
                  })
                }
              />
            </label>
            <label>
              {t("최대 토큰")}
              <input
                min={128}
                max={32768}
                type="number"
                value={settings.aiMaxTokens ?? 2048}
                onChange={(event) =>
                  onChange({
                    ...settings,
                    aiMaxTokens: Number(event.target.value),
                  })
                }
              />
            </label>
          </div>
          <div className="settings-warning">
            {remote
              ? t("선택한 문서의 필요한 일부가 설정한 외부 AI 서비스로 전송됩니다. 전송 전에 사용자 확인을 받습니다.")
              : t("Ollama는 이 PC에서 실행됩니다. EveryFile은 사용자가 선택한 모델만 사용하며 다른 제공자로 자동 전환하지 않습니다.")}
          </div>
        </>
      )}
    </div>
  );
}
