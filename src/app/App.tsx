import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ResizablePane } from "../components/ResizablePane";
import { IndexStatusController } from "../features/folders/IndexStatus";
import { PreviewPanel } from "../features/preview/PreviewPanel";
import { SearchWorkspace } from "../features/search/SearchWorkspace";
import type { FolderRecord } from "../lib/types";
import "../styles/app.css";
import { Header } from "./Header";
import { defaultLocale, productTranslations, type Locale } from "./translations";

const LEFT_PANE_KEY = "everyfile.ui.left-pane-width";
const RIGHT_PANE_KEY = "everyfile.ui.right-pane-width";
const LEFT_PANE_DEFAULT = 260;
const LEFT_PANE_MIN = 208;
const LEFT_PANE_MAX = 420;
const RIGHT_PANE_MIN = 280;
const RIGHT_PANE_MAX = 720;
const CENTER_PANE_MIN = 520;
const PREVIEW_BREAKPOINT = 1100;
const COMPACT_HEADER_BREAKPOINT = 560;
const APP_VERSION = "v0.1.0";

export interface AppProps {
  folders?: FolderRecord[];
  indexedDocumentCount?: number;
  queueState?: "idle" | "indexing" | "paused" | "error";
  selectedDocumentId?: string | null;
  onAddFolder?: () => void;
  onSettings?: () => void;
  onStatistics?: () => void;
  onDocumentSelect?: (documentId: string) => void;
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

function FolderPane({ folders }: { folders: FolderRecord[] }) {
  return (
    <aside className="folder-pane" aria-label="등록 폴더 / Indexed folders">
      <div className="pane-heading">
        <h2>등록 폴더</h2>
        <span className="folder-count">{folders.length}</span>
      </div>
      {folders.length === 0 ? (
        <div className="folder-empty">
          선택한 폴더만 이 PC에서 색인됩니다.
          <br />
          폴더 추가 버튼으로 시작하세요.
        </div>
      ) : (
        <div className="folder-list">
          {folders.map((folder) => (
            <button className="folder-item" key={folder.id} type="button">
              <span>{folder.displayName}</span>
              <span className="folder-count">
                {folder.documentCount.toLocaleString()}
              </span>
            </button>
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
  onSettings,
  onStatistics,
  onDocumentSelect,
}: AppProps) {
  const [locale, setLocale] = useState<Locale>(defaultLocale);
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [workspaceSelectedDocumentId, setWorkspaceSelectedDocumentId] =
    useState(selectedDocumentId);
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
  const previewRequested = workspaceWidth > PREVIEW_BREAKPOINT;
  const leftCanFit = workspaceWidth >= LEFT_PANE_MIN + CENTER_PANE_MIN;
  const previewCanFit =
    previewRequested &&
    workspaceWidth >=
      CENTER_PANE_MIN + RIGHT_PANE_MIN + (sidebarOpen ? LEFT_PANE_MIN : 0);
  const leftMaximumForLayout = Math.max(
    LEFT_PANE_MIN,
    Math.min(
      LEFT_PANE_MAX,
      workspaceWidth -
        CENTER_PANE_MIN -
        (previewCanFit ? RIGHT_PANE_MIN : 0),
    ),
  );
  const leftPaneVisible = sidebarOpen && leftCanFit;
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
    const handleShortcut = (event: KeyboardEvent) => {
      const textEntry = isTextEntryTarget(event.target);
      if (
        event.ctrlKey &&
        !event.altKey &&
        !event.metaKey &&
        event.key.toLowerCase() === "b" &&
        !textEntry
      ) {
        event.preventDefault();
        setSidebarOpen((open) => !open);
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
        onLocaleChange={setLocale}
        onHome={() => {
          setSidebarOpen(true);
          searchInput.current?.focus();
        }}
        onSettings={onSettings}
        onStatistics={onStatistics}
      />
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
          <FolderPane folders={folders} />
        </ResizablePane>

        <section className="center-pane" aria-label="검색 작업 공간 / Search workspace">
          <SearchWorkspace
            folders={folders}
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
          <PreviewPanel documentId={workspaceSelectedDocumentId} />
        </ResizablePane>
      </main>
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
