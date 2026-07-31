import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ResizablePane } from "../components/ResizablePane";
import {
  CommandStatus,
  type CommandStatusMessage,
} from "../components/CommandStatus";
import { IndexStatusController } from "../features/folders/IndexStatus";
import { PreviewPanel } from "../features/preview/PreviewPanel";
import { SearchWorkspace } from "../features/search/SearchWorkspace";
import { SettingsDialog } from "../features/settings/SettingsDialog";
import { StatisticsDialog } from "../features/statistics/StatisticsDialog";
import type {
  AppSettings,
  FolderRecord,
  StatisticsSearchFilter,
} from "../lib/types";
import { getSettings, saveSettings } from "../lib/ipc";
import "../styles/app.css";
import { Header } from "./Header";
import { defaultLocale, productTranslations, type Locale } from "./translations";

const LEFT_PANE_KEY = "everyfile.ui.left-pane-width";
const RIGHT_PANE_KEY = "everyfile.ui.right-pane-width";
const LEFT_PANE_VISIBLE_KEY = "everyfile.ui.left-pane-visible";
const RIGHT_PANE_VISIBLE_KEY = "everyfile.ui.right-pane-visible";
const LEFT_PANE_DEFAULT = 260;
const LEFT_PANE_MIN = 208;
const LEFT_PANE_MAX = 420;
const RIGHT_PANE_MIN = 280;
const RIGHT_PANE_MAX = 720;
const CENTER_PANE_MIN = 520;
const PREVIEW_BREAKPOINT = 1100;
const COMPACT_HEADER_BREAKPOINT = 560;
const APP_VERSION = "v1.0.0";

export interface AppProps {
  folders?: FolderRecord[];
  indexedDocumentCount?: number;
  queueState?: "idle" | "indexing" | "paused" | "error";
  selectedDocumentId?: string | null;
  onAddFolder?: () => void;
  onRemoveFolder?: (folderId: string) => void;
  onSettings?: () => void;
  onStatistics?: () => void;
  onDocumentSelect?: (documentId: string) => void;
  commandStatus?: CommandStatusMessage | null;
  onDismissCommandStatus?: () => void;
}

function readPersistedVisibility(key: string, fallback: boolean) {
  try {
    const value = window.localStorage.getItem(key);
    if (value === "true") return true;
    if (value === "false") return false;
  } catch {
    // Retain the safe default when browser storage is unavailable.
  }
  return fallback;
}

function usePersistedVisibility(key: string, fallback: boolean) {
  const [visible, setVisible] = useState(() =>
    readPersistedVisibility(key, fallback),
  );
  const updateVisible = (next: boolean | ((current: boolean) => boolean)) => {
    setVisible((current) => {
      const value = typeof next === "function" ? next(current) : next;
      try {
        window.localStorage.setItem(key, String(value));
      } catch {
        // Keep the in-memory preference when browser storage is unavailable.
      }
      return value;
    });
  };
  return [visible, updateVisible] as const;
}

function readPersistedWidth(key: string, fallback: number, min: number, max: number) {
  try {
    const storedValue = Number(window.localStorage.getItem(key));
    if (Number.isFinite(storedValue) && storedValue >= min && storedValue <= max) {
      return storedValue;
    }
  } catch {
    // Local settings can be unavailable in hardened browser previews.
  }
  return fallback;
}

function defaultRightPaneWidth() {
  const usableWidth = Math.max(0, window.innerWidth - LEFT_PANE_DEFAULT);
  return Math.min(
    RIGHT_PANE_MAX,
    Math.max(RIGHT_PANE_MIN, Math.round(usableWidth * 0.38)),
  );
}

function usePersistedWidth(key: string, fallback: number, min: number, max: number) {
  const [width, setWidth] = useState(() =>
    readPersistedWidth(key, fallback, min, max),
  );

  const updateWidth = (nextWidth: number) => {
    const clampedWidth = Math.min(max, Math.max(min, Math.round(nextWidth)));
    setWidth(clampedWidth);
    try {
      window.localStorage.setItem(key, String(clampedWidth));
    } catch {
      // Keep the in-memory width if local settings cannot be written.
    }
  };

  return [width, updateWidth] as const;
}

function useWorkspaceWidth() {
  const workspaceRef = useRef<HTMLElement>(null);
  const [workspaceWidth, setWorkspaceWidth] = useState(() => window.innerWidth);

  useLayoutEffect(() => {
    const workspace = workspaceRef.current;
    if (!workspace) return;

    const updateWidth = (width: number) => {
      if (Number.isFinite(width) && width > 0) {
        setWorkspaceWidth(Math.floor(width));
      }
    };
    const measure = () => {
      const measuredWidth = workspace.getBoundingClientRect().width;
      updateWidth(measuredWidth || window.innerWidth);
    };

    measure();
    if (typeof ResizeObserver === "function") {
      const observer = new ResizeObserver((entries) => {
        const entry = entries[0];
        if (entry) updateWidth(entry.contentRect.width);
      });
      observer.observe(workspace);
      return () => observer.disconnect();
    }

    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, []);

  return [workspaceRef, workspaceWidth] as const;
}

function isTextEntryTarget(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target.isContentEditable ||
    target.closest('[contenteditable]:not([contenteditable="false"])') !== null ||
    ["INPUT", "SELECT", "TEXTAREA"].includes(target.tagName)
  );
}

function FolderPane({
  folders,
  onAddFolder,
  onRemoveFolder,
}: {
  folders: FolderRecord[];
  onAddFolder?: () => void;
  onRemoveFolder?: (folderId: string) => void;
}) {
  return (
    <aside className="folder-pane" aria-label="등록 폴더 / Indexed folders">
      <div className="pane-heading">
        <h2>등록 폴더</h2>
        <span className="folder-count">{folders.length}</span>
      </div>
      {folders.length === 0 ? (
        <div className="folder-empty">
          <button type="button" onClick={onAddFolder} disabled={!onAddFolder}>
            폴더 추가
          </button>
          <br />
          선택한 폴더만 이 PC에서 색인됩니다.
          <br />
          폴더 추가 버튼으로 시작하세요.
        </div>
      ) : (
        <div className="folder-list">
          {folders.map((folder) => (
            <div className="folder-item" key={folder.id}>
              <span title={folder.canonicalPath}>{folder.displayName}</span>
              <span className="folder-count">
                {folder.documentCount.toLocaleString()}
              </span>
              {onRemoveFolder && (
                <button
                  type="button"
                  className="folder-remove"
                  aria-label={`${folder.displayName} 등록 해제`}
                  onClick={() => onRemoveFolder(folder.id)}
                >
                  ×
                </button>
              )}
            </div>
          ))}
        </div>
      )}
    </aside>
  );
}

const queueLabels = {
  idle: "대기",
  indexing: "색인 중",
  paused: "일시 정지",
  error: "확인 필요",
} as const;

export function App({
  folders = [],
  indexedDocumentCount = 0,
  queueState = "idle",
  selectedDocumentId = null,
  onAddFolder,
  onRemoveFolder,
  onSettings,
  onStatistics,
  onDocumentSelect,
  commandStatus = null,
  onDismissCommandStatus,
}: AppProps) {
  const [locale, setLocale] = useState<Locale>(defaultLocale);
  const [leftPanelOpen, setLeftPanelOpen] = usePersistedVisibility(
    LEFT_PANE_VISIBLE_KEY,
    true,
  );
  const [rightPanelOpen, setRightPanelOpen] = usePersistedVisibility(
    RIGHT_PANE_VISIBLE_KEY,
    true,
  );
  const [workspaceSelectedDocumentId, setWorkspaceSelectedDocumentId] =
    useState(selectedDocumentId);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [statisticsOpen, setStatisticsOpen] = useState(false);
  const [statisticsFilter, setStatisticsFilter] =
    useState<StatisticsSearchFilter | null>(null);
  const [historyQuery, setHistoryQuery] = useState<string | null>(null);
  const [appSettings, setAppSettings] = useState<AppSettings | null>(null);
  const searchInput = useRef<HTMLInputElement>(null);
  const [leftWidth, setLeftWidth] = usePersistedWidth(
    LEFT_PANE_KEY,
    LEFT_PANE_DEFAULT,
    LEFT_PANE_MIN,
    LEFT_PANE_MAX,
  );
  const [rightWidth, setRightWidth] = usePersistedWidth(
    RIGHT_PANE_KEY,
    defaultRightPaneWidth(),
    RIGHT_PANE_MIN,
    RIGHT_PANE_MAX,
  );
  const [workspaceRef, workspaceWidth] = useWorkspaceWidth();
  const { tagline } = productTranslations[locale];
  const previewRequested =
    rightPanelOpen &&
    (workspaceWidth > PREVIEW_BREAKPOINT || !leftPanelOpen);
  const leftCanFit = workspaceWidth >= LEFT_PANE_MIN + CENTER_PANE_MIN;
  const previewCanFit =
    previewRequested &&
    workspaceWidth >=
      CENTER_PANE_MIN + RIGHT_PANE_MIN + (leftPanelOpen ? LEFT_PANE_MIN : 0);
  const leftMaximumForLayout = Math.max(
    LEFT_PANE_MIN,
    Math.min(
      LEFT_PANE_MAX,
      workspaceWidth -
        CENTER_PANE_MIN -
        (previewCanFit ? RIGHT_PANE_MIN : 0),
    ),
  );
  const leftPaneVisible = leftPanelOpen && leftCanFit;
  const renderedLeftWidth = leftPaneVisible
    ? Math.min(leftWidth, leftMaximumForLayout)
    : leftWidth;
  const rightMaximumForLayout = Math.max(
    RIGHT_PANE_MIN,
    Math.min(
      RIGHT_PANE_MAX,
      workspaceWidth -
        CENTER_PANE_MIN -
        (leftPaneVisible ? renderedLeftWidth : 0),
    ),
  );
  const previewVisible = previewCanFit;
  const renderedRightWidth = previewVisible
    ? Math.min(rightWidth, rightMaximumForLayout)
    : rightWidth;
  const compactHeader = workspaceWidth <= COMPACT_HEADER_BREAKPOINT;

  useEffect(() => {
    setWorkspaceSelectedDocumentId(selectedDocumentId);
  }, [selectedDocumentId]);

  useEffect(() => {
    void getSettings()
      .then((settings) => {
        setAppSettings(settings);
        if (settings.language === "ko" || settings.language === "en") {
          setLocale(settings.language);
        }
      })
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    const handleShortcut = (event: KeyboardEvent) => {
      const textEntry = isTextEntryTarget(event.target);
      if (
        event.ctrlKey &&
        !event.shiftKey &&
        !event.altKey &&
        !event.metaKey &&
        event.key.toLowerCase() === "b" &&
        !textEntry
      ) {
        event.preventDefault();
        setLeftPanelOpen((open) => !open);
        return;
      }
      if (
        event.ctrlKey &&
        event.shiftKey &&
        !event.altKey &&
        !event.metaKey &&
        event.key.toLowerCase() === "b" &&
        !textEntry
      ) {
        event.preventDefault();
        setRightPanelOpen((open) => !open);
        return;
      }
      if (
        event.key === "/" &&
        !event.ctrlKey &&
        !event.altKey &&
        !event.metaKey &&
        !textEntry
      ) {
        event.preventDefault();
        searchInput.current?.focus();
      }
    };

    window.addEventListener("keydown", handleShortcut);
    return () => window.removeEventListener("keydown", handleShortcut);
  }, []);

  return (
    <div className="app-shell">
      <Header
        compact={compactHeader}
        locale={locale}
        tagline={tagline}
        onAddFolder={onAddFolder}
        leftPanelOpen={leftPanelOpen}
        rightPanelOpen={rightPanelOpen}
        onToggleLeftPanel={() => setLeftPanelOpen((open) => !open)}
        onToggleRightPanel={() => setRightPanelOpen((open) => !open)}
        onLocaleChange={(nextLocale) => {
          setLocale(nextLocale);
          if (appSettings) {
            const nextSettings = { ...appSettings, language: nextLocale };
            setAppSettings(nextSettings);
            void saveSettings(nextSettings).catch(() => undefined);
          }
        }}
        onHome={() => {
          setLeftPanelOpen(true);
          searchInput.current?.focus();
        }}
        onSettings={() => {
          onSettings?.();
          setSettingsOpen(true);
        }}
        onStatistics={() => {
          onStatistics?.();
          setStatisticsOpen(true);
        }}
      />
      <div className="command-status-slot">
        <CommandStatus
          message={commandStatus}
          onDismiss={onDismissCommandStatus}
        />
      </div>
      <main ref={workspaceRef} className="workspace">
        <ResizablePane
          className="left-pane-container"
          hidden={!leftPaneVisible}
          label="폴더 패널 크기 조절 / Resize folder pane"
          maxWidth={leftMaximumForLayout}
          minWidth={LEFT_PANE_MIN}
          onWidthChange={setLeftWidth}
          resizeEdge="right"
          width={renderedLeftWidth}
        >
          <FolderPane
            folders={folders}
            onAddFolder={onAddFolder}
            onRemoveFolder={onRemoveFolder}
          />
        </ResizablePane>

        <section className="center-pane" aria-label="검색 작업 공간 / Search workspace">
          <SearchWorkspace
            folders={folders}
            historyQuery={historyQuery}
            statisticsFilter={statisticsFilter}
            pageSize={appSettings?.resultPageSize}
            fileClickBehavior={appSettings?.fileClickBehavior}
            dateDisplay={appSettings?.dateDisplay}
            onSelectDocument={(documentId) => {
              setWorkspaceSelectedDocumentId(documentId);
              onDocumentSelect?.(documentId);
            }}
            ref={searchInput}
          />
          <IndexStatusController />
        </section>

        <ResizablePane
          className="preview-pane-container"
          hidden={!previewVisible}
          label="미리보기 패널 크기 조절 / Resize preview pane"
          maxWidth={rightMaximumForLayout}
          minWidth={RIGHT_PANE_MIN}
          onWidthChange={setRightWidth}
          resizeEdge="left"
          width={renderedRightWidth}
        >
          <PreviewPanel
            documentId={workspaceSelectedDocumentId}
            aiEnabled={appSettings?.aiEnabled ?? false}
            aiProvider={appSettings?.aiProvider ?? "ollama"}
          />
        </ResizablePane>
      </main>
      <SettingsDialog
        folders={folders}
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        onSaved={(settings) => {
          setAppSettings(settings);
          if (settings.language === "ko" || settings.language === "en") {
            setLocale(settings.language);
          }
        }}
      />
      <StatisticsDialog
        open={statisticsOpen}
        registeredFolderIds={folders.map((folder) => folder.id)}
        onClose={() => setStatisticsOpen(false)}
        onApplyFilter={(filter) => {
          setStatisticsFilter(filter);
          setHistoryQuery(null);
        }}
        onSearchHistory={(query) => {
          setHistoryQuery(query);
          setStatisticsFilter(null);
          setStatisticsOpen(false);
        }}
      />
      <footer className="app-status">
        <div className="status-summary" role="status" aria-live="polite">
          <span>색인 문서 {indexedDocumentCount.toLocaleString()}개</span>
          <span>폴더 {folders.length.toLocaleString()}개</span>
          <span>대기열 {queueLabels[queueState]}</span>
          <span className="app-version">{APP_VERSION}</span>
        </div>
      </footer>
    </div>
  );
}
