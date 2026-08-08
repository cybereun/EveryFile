import { useCallback, useEffect, useState } from "react";
import { useModalDialog } from "../../components/useModalDialog";
import {
  getSettings,
  getAiSecretStatus,
  getDiagnosticsLogFolder,
  listParseErrors,
  resetApplicationData as resetApplicationDataCommand,
  retryParse,
  saveAiSecret,
  saveSettings,
} from "../../lib/ipc";
import type { AppSettings, FolderRecord, ParseErrorRecord } from "../../lib/types";
import type { AvailableUpdate } from "../../lib/updater";
import { DiagnosticsSettings } from "./DiagnosticsSettings";
import { GeneralSettings } from "./GeneralSettings";
import { AiSettings } from "./AiSettings";
import { SearchSettings } from "./SearchSettings";
import { SystemSettings } from "./SystemSettings";
import { useI18n } from "../../app/translations";

type SettingsTab = "general" | "search" | "ai" | "system" | "diagnostics";

const tabs: { id: SettingsTab; label: string }[] = [
  { id: "general", label: "일반" },
  { id: "search", label: "검색" },
  { id: "ai", label: "AI" },
  { id: "system", label: "시스템" },
  { id: "diagnostics", label: "진단" },
];

export interface SettingsDialogProps {
  open: boolean;
  folders?: FolderRecord[];
  onClose: () => void;
  loadSettings?: () => Promise<AppSettings>;
  persistSettings?: (settings: AppSettings) => Promise<AppSettings>;
  loadParseErrors?: () => Promise<ParseErrorRecord[]>;
  retryDocument?: (documentId: string) => Promise<void>;
  resetApplicationData?: () => Promise<void>;
  loadDiagnosticsLogFolder?: () => Promise<string>;
  onCheckForUpdates?: () => Promise<AvailableUpdate | null>;
  onSaved?: (settings: AppSettings) => void;
}

export function SettingsDialog({
  open,
  folders = [],
  onClose,
  loadSettings = getSettings,
  persistSettings = saveSettings,
  loadParseErrors = listParseErrors,
  retryDocument = retryParse,
  resetApplicationData = resetApplicationDataCommand,
  loadDiagnosticsLogFolder = getDiagnosticsLogFolder,
  onCheckForUpdates,
  onSaved,
}: SettingsDialogProps) {
  const { t } = useI18n();
  const [activeTab, setActiveTab] = useState<SettingsTab>("general");
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [errors, setErrors] = useState<ParseErrorRecord[]>([]);
  const [message, setMessage] = useState("");
  const [logFolder, setLogFolder] = useState<string>();
  const [resetConfirmationOpen, setResetConfirmationOpen] = useState(false);
  const [hasSavedSecret, setHasSavedSecret] = useState(false);
  const [secretDraft, setSecretDraft] = useState<string | null>(null);
  const [updateChecking, setUpdateChecking] = useState(false);
  const [updateStatus, setUpdateStatus] = useState("");
  const close = useCallback(() => onClose(), [onClose]);
  const dialogRef = useModalDialog(open && !resetConfirmationOpen, close);

  useEffect(() => {
    if (!open) return;
    setMessage("");
    setUpdateStatus("");
    void loadSettings().then(setSettings).catch(() => setMessage(t("설정을 불러오지 못했습니다.")));
    void loadParseErrors().then(setErrors).catch(() => setErrors([]));
    void loadDiagnosticsLogFolder().then(setLogFolder).catch(() => setLogFolder(undefined));
  }, [loadDiagnosticsLogFolder, loadParseErrors, loadSettings, open, t]);

  useEffect(() => {
    if (!open || !settings || (settings.aiProvider ?? "ollama") === "ollama") {
      setHasSavedSecret(false);
      setSecretDraft(null);
      return;
    }
    setSecretDraft(null);
    void getAiSecretStatus(settings.aiProvider ?? "gemini")
      .then(setHasSavedSecret)
      .catch(() => setHasSavedSecret(false));
  }, [open, settings?.aiProvider]);

  if (!open) return null;

  const checkForUpdates = async () => {
    if (!onCheckForUpdates) return null;
    setUpdateChecking(true);
    setUpdateStatus("");
    try {
      const update = await onCheckForUpdates();
      setUpdateStatus(
        update
          ? t("새 버전 {version}을(를) 찾았습니다. 설치 창을 확인하세요.", { version: update.version })
          : t("현재 최신 버전입니다."),
      );
      return update;
    } catch {
      setUpdateStatus(t("업데이트를 확인하지 못했습니다. 잠시 후 다시 시도하세요."));
      return null;
    } finally {
      setUpdateChecking(false);
    }
  };

  const moveTab = (direction: number) => {
    const index = tabs.findIndex((tab) => tab.id === activeTab);
    const next = tabs[(index + direction + tabs.length) % tabs.length].id;
    setActiveTab(next);
    window.requestAnimationFrame(() =>
      document.getElementById(`settings-tab-${next}`)?.focus(),
    );
  };

  return (
    <div className="modal-backdrop">
      <div
        ref={dialogRef}
        className="app-dialog settings-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-title"
        aria-hidden={resetConfirmationOpen || undefined}
        inert={resetConfirmationOpen || undefined}
      >
        <header className="dialog-header">
          <h2 id="settings-title">{t("설정")}</h2>
          <button type="button" aria-label={t("설정 닫기")} onClick={close}>×</button>
        </header>
        <div
          className="dialog-tabs"
          role="tablist"
          aria-label={t("설정 항목")}
          onKeyDown={(event) => {
            if (event.key === "ArrowRight") {
              event.preventDefault();
              moveTab(1);
            }
            if (event.key === "ArrowLeft") {
              event.preventDefault();
              moveTab(-1);
            }
          }}
        >
          {tabs.map((tab) => (
            <button
              key={tab.id}
              type="button"
              role="tab"
              id={`settings-tab-${tab.id}`}
              aria-selected={activeTab === tab.id}
              aria-controls={`settings-panel-${tab.id}`}
              tabIndex={activeTab === tab.id ? 0 : -1}
              onClick={() => setActiveTab(tab.id)}
            >
              {t(tab.label)}
            </button>
          ))}
        </div>
        <div
          id={`settings-panel-${activeTab}`}
          aria-labelledby={`settings-tab-${activeTab}`}
          className="dialog-body"
          role="tabpanel"
          tabIndex={0}
        >
          {!settings ? (
            <p role="status">{message || t("설정을 불러오는 중…")}</p>
          ) : activeTab === "general" ? (
            <GeneralSettings settings={settings} onChange={setSettings} />
          ) : activeTab === "search" ? (
            <SearchSettings settings={settings} folders={folders} onChange={setSettings} />
          ) : activeTab === "ai" ? (
            <AiSettings
              settings={settings}
              hasSavedSecret={hasSavedSecret}
              secretDraft={secretDraft}
              onChange={setSettings}
              onSecretDraftChange={setSecretDraft}
            />
          ) : activeTab === "system" ? (
            <SystemSettings
              settings={settings}
              onChange={setSettings}
              onReset={resetApplicationData}
              onConfirmationChange={setResetConfirmationOpen}
            />
          ) : (
            <DiagnosticsSettings
              settings={settings}
              errors={errors}
              logFolder={logFolder}
              onChange={setSettings}
              onCheckForUpdates={checkForUpdates}
              updateChecking={updateChecking}
              updateStatus={updateStatus}
              onRetry={async (documentId) => {
                await retryDocument(documentId);
                setErrors((current) =>
                  current.filter((error) => error.documentId !== documentId),
                );
              }}
            />
          )}
        </div>
        <footer className="dialog-footer">
          <span role="status" aria-live="polite">{message}</span>
          <button type="button" onClick={close}>{t("닫기")}</button>
          <button
            type="button"
            className="primary-button"
            disabled={!settings}
            onClick={async () => {
              if (!settings) return;
              try {
                const saved = await persistSettings({
                  ...settings,
                });
                if (
                  saved.aiProvider !== "ollama" &&
                  secretDraft !== null
                ) {
                  await saveAiSecret(saved.aiProvider ?? "gemini", secretDraft);
                  setHasSavedSecret(secretDraft.trim().length > 0);
                  setSecretDraft(null);
                }
                setSettings(saved);
                onSaved?.(saved);
                setMessage(t("저장했습니다."));
              } catch {
                setMessage(t("설정을 저장하지 못했습니다."));
              }
            }}
          >
            {t("저장")}
          </button>
        </footer>
      </div>
    </div>
  );
}
