import { useEffect, useRef, useState } from "react";
import { cancelDocumentAi, runDocumentAi } from "../../lib/ipc";

interface DocumentAiPanelProps {
  documentId: string;
  mode: "summary" | "question";
  provider: "ollama" | "gemini" | "openai";
  onClose: () => void;
  runApi?: typeof runDocumentAi;
  cancelApi?: typeof cancelDocumentAi;
}

function createRequestId() {
  return globalThis.crypto?.randomUUID?.() ?? `ai-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

export function DocumentAiPanel({
  documentId,
  mode,
  provider,
  onClose,
  runApi = runDocumentAi,
  cancelApi = cancelDocumentAi,
}: DocumentAiPanelProps) {
  const [question, setQuestion] = useState("");
  const [consent, setConsent] = useState(false);
  const [answer, setAnswer] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const requestId = useRef<string | null>(null);
  const remote = provider !== "ollama";

  useEffect(
    () => () => {
      if (requestId.current) void cancelApi(requestId.current);
    },
    [cancelApi],
  );

  const cancel = async () => {
    const active = requestId.current;
    if (!active) return;
    await cancelApi(active);
  };

  const submit = async () => {
    const active = createRequestId();
    requestId.current = active;
    setLoading(true);
    setAnswer("");
    setError("");
    try {
      const result = await runApi(
        active,
        documentId,
        mode === "question" ? question : null,
        !remote || consent,
      );
      if (requestId.current === active) setAnswer(result);
    } catch (caught) {
      if (requestId.current === active) {
        setError(caught instanceof Error ? caught.message : "AI 요청에 실패했습니다.");
      }
    } finally {
      if (requestId.current === active) {
        requestId.current = null;
        setLoading(false);
      }
    }
  };

  return (
    <section className="document-ai-panel" aria-label="문서 AI">
      <header>
        <strong>{mode === "summary" ? "AI 요약" : "이 파일에 대한 질문"}</strong>
        <button type="button" onClick={onClose} aria-label="AI 패널 닫기">
          ×
        </button>
      </header>
      {mode === "question" && (
        <textarea
          aria-label="문서에 대한 질문"
          placeholder="이 문서에서 무엇을 알고 싶나요?"
          value={question}
          onChange={(event) => setQuestion(event.target.value)}
        />
      )}
      {remote && (
        <label className="ai-consent">
          <input
            type="checkbox"
            checked={consent}
            onChange={(event) => setConsent(event.target.checked)}
          />
          선택한 문서의 필요한 일부를 {provider === "gemini" ? "Google Gemini" : "OpenAI"}로
          전송하는 데 동의합니다.
        </label>
      )}
      <div className="document-ai-actions">
        <button
          type="button"
          className="primary-button"
          disabled={
            loading ||
            (mode === "question" && !question.trim()) ||
            (remote && !consent)
          }
          onClick={() => void submit()}
        >
          {loading ? "생성 중…" : "실행"}
        </button>
        {loading && (
          <button type="button" onClick={() => void cancel()}>
            취소
          </button>
        )}
      </div>
      {error && <div className="preview-inline-error" role="alert">{error}</div>}
      {answer && <div className="ai-answer" aria-live="polite">{answer}</div>}
    </section>
  );
}
