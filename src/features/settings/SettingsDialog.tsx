import { useCallback, useEffect, useState } from "react";
import { useModalDialog } from "../../components/useModalDialog";
import {
  getSettings,
  getDiagnosticsLogFolder,
  listParseErrors,
  resetApplicationData as resetApplicationDataCommand,
  retryParse,
  saveSettings,
} from "../../lib/ipc";
import type { AppSettings, FolderRecord, ParseErrorRecord } from "../../lib/types";
import { DiagnosticsSettings } from "./DiagnosticsSettings";
import { GeneralSettings } from "./GeneralSettings";
import { SearchSettings } from "./SearchSettings";
import { SystemSettings } from "./SystemSettings";

type SettingsTab = "general" | "search" | "system" | "diagnostics";

const tabs: { id: SettingsTab; label: string }[] = [
  { id: "general", label: "일반" },
  { id: "search", label: "검색" },
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
  onSaved,
}: SettingsDialogProps) {
  const [activeTab, setActiveTab] = useState<SettingsTab>("general");
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [errors, setErrors] = useState<ParseErrorRecord[]>([]);
  const [message, setMessage] = useState("");
  const [logFolder, setLogFolder] = useState<string>();
  const close = useCallback(() => onClose(), [onClose]);
  const dialogRef = useModalDialog(open, close);

  useEffect(() => {
    if (!open) return;
    setMessage("");
    void loadSettings().then(setSettings).catch(() => setMessage("설정을 불러오지 못했습니다."));
    void loadParseErrors().then(setErrors).catch(() => setErrors([]));
    void loadDiagnosticsLogFolder().then(setLogFolder).catch(() => setLogFolder(undefined));
  }, [loadDiagnosticsLogFolder, loadParseErrors, loadSettings, open]);

  if (!open) return null;

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
      >
        <header className="dialog-header">
          <h2 id="settings-title">설정</h2>
          <button type="button" aria-label="설정 닫기" onClick={close}>×</button>
        </header>
        <div
          className="dialog-tabs"
          role="tablist"
          aria-label="설정 항목"
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
              {tab.label}
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
            <p role="status">{message || "설정을 불러오는 중…"}</p>
          ) : activeTab === "general" ? (
            <GeneralSettings settings={settings} onChange={setSettings} />
          ) : activeTab === "search" ? (
            <SearchSettings settings={settings} folders={folders} onChange={setSettings} />
          ) : activeTab === "system" ? (
            <SystemSettings
              settings={settings}
              onChange={setSettings}
              onReset={resetApplicationData}
            />
          ) : (
            <DiagnosticsSettings
              errors={errors}
              logFolder={logFolder}
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
          <button type="button" onClick={close}>닫기</button>
          <button
            type="button"
            className="primary-button"
            disabled={!settings}
            onClick={async () => {
              if (!settings) return;
              try {
                const saved = await persistSettings(settings);
                setSettings(saved);
                onSaved?.(saved);
                setMessage("저장했습니다.");
              } catch {
                setMessage("설정을 저장하지 못했습니다.");
              }
            }}
          >
            저장
          </button>
        </footer>
      </div>
    </div>
  );
}
