import { useI18n } from "../app/translations";

export type CommandStatusKind = "info" | "success" | "error";

export interface CommandStatusMessage {
  kind: CommandStatusKind;
  text: string;
}

interface CommandStatusProps {
  message: CommandStatusMessage | null;
  onDismiss?: () => void;
}

export function CommandStatus({ message, onDismiss }: CommandStatusProps) {
  const { t } = useI18n();
  if (!message) return null;

  return (
    <div
      className={`command-status command-status--${message.kind}`}
      role={message.kind === "error" ? "alert" : "status"}
      aria-live={message.kind === "error" ? "assertive" : "polite"}
    >
      <span>{message.text}</span>
      {onDismiss && (
        <button type="button" onClick={onDismiss} aria-label={`${t("알림 닫기")} / Dismiss`}>
          ×
        </button>
      )}
    </div>
  );
}
