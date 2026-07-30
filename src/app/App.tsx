import { useEffect, useRef, useState } from "react";
import { ResizablePane } from "../components/ResizablePane";
import { IndexStatusController } from "../features/folders/IndexStatus";
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
const APP_VERSION = "v0.1.0";

export interface AppProps {
  folders?: FolderRecord[];
  indexedDocumentCount?: number;
  queueState?: "idle" | "indexing" | "paused" | "error";
  selectedDocumentId?: string | null;
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

function useWidePreview() {
  const query = "(min-width: 1101px)";
  const [widePreview, setWidePreview] = useState(() =>
    typeof window.matchMedia === "function" ? window.matchMedia(query).matches : true,
  );

  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const mediaQuery = window.matchMedia(query);
    const update = (event: MediaQueryListEvent) => setWidePreview(event.matches);
    setWidePreview(mediaQuery.matches);
    mediaQuery.addEventListener("change", update);
    return () => mediaQuery.removeEventListener("change", update);
  }, []);

  return widePreview;
}

function isTextEntryTarget(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target.isContentEditable ||
    ["INPUT", "SELECT", "TEXTAREA"].includes(target.tagName)
  );
}

function SearchIcon() {
  return (
    <svg
      aria-hidden="true"
      fill="none"
      stroke="currentColor"
      strokeLinecap="round"
      strokeWidth="2"
      viewBox="0 0 24 24"
    >
      <circle cx="11" cy="11" r="7" />
      <path d="m16 16 5 5" />
    </svg>
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

function PreviewPane({ selectedDocumentId }: { selectedDocumentId: string | null }) {
  return (
    <section
      className="preview-pane"
      aria-label="문서 미리보기 / Document preview"
    >
      <div className="pane-heading">
        <h2>미리보기</h2>
      </div>
      <div className="preview-card">
        {selectedDocumentId ? (
          <>
            <strong>선택한 문서</strong>
            <p>
              문서 ID: {selectedDocumentId}
              <br />
              파싱된 문서 내용은 이 영역에서 안전한 텍스트로 표시됩니다.
            </p>
          </>
        ) : (
          <p>검색 결과에서 파일을 선택하면 이곳에서 내용을 확인할 수 있습니다.</p>
        )}
      </div>
    </section>
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
}: AppProps) {
  const [locale, setLocale] = useState<Locale>(defaultLocale);
  const [sidebarOpen, setSidebarOpen] = useState(true);
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
  const widePreview = useWidePreview();
  const { tagline } = productTranslations[locale];

  useEffect(() => {
    const handleShortcut = (event: KeyboardEvent) => {
      if (event.ctrlKey && !event.altKey && !event.metaKey && event.key.toLowerCase() === "b") {
        event.preventDefault();
        setSidebarOpen((open) => !open);
        return;
      }
      if (
        event.key === "/" &&
        !event.ctrlKey &&
        !event.altKey &&
        !event.metaKey &&
        !isTextEntryTarget(event.target)
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
        locale={locale}
        tagline={tagline}
        onLocaleChange={setLocale}
        onHome={() => {
          setSidebarOpen(true);
          searchInput.current?.focus();
        }}
      />
      <main className="workspace">
        <ResizablePane
          className="left-pane-container"
          hidden={!sidebarOpen}
          label="폴더 패널 크기 조절 / Resize folder pane"
          maxWidth={LEFT_PANE_MAX}
          minWidth={LEFT_PANE_MIN}
          onWidthChange={setLeftWidth}
          resizeEdge="right"
          width={leftWidth}
        >
          <FolderPane folders={folders} />
        </ResizablePane>

        <section className="center-pane" aria-label="검색 작업 공간 / Search workspace">
          <section
            className="search-panel"
            role="search"
            aria-label="파일 검색 / File search"
          >
            <h2>내 파일에서 찾기</h2>
            <label className="search-field">
              <SearchIcon />
              <span className="sr-only">검색어 / Search query</span>
              <input
                ref={searchInput}
                type="search"
                placeholder="파일명이나 문서 속 단어를 입력하세요"
              />
              <kbd className="shortcut-hint" aria-hidden="true">
                /
              </kbd>
            </label>
          </section>
          <div className="workspace-empty">
            <div>
              <strong>Anything in your files.</strong>
              <p>
                파일 이름과 문서 내용을 한곳에서 빠르게 검색하세요. 검색 데이터는
                이 PC 안에 머뭅니다.
              </p>
            </div>
          </div>
          <IndexStatusController />
        </section>

        <ResizablePane
          className="preview-pane-container"
          hidden={!widePreview}
          label="미리보기 패널 크기 조절 / Resize preview pane"
          maxWidth={RIGHT_PANE_MAX}
          minWidth={RIGHT_PANE_MIN}
          onWidthChange={setRightWidth}
          resizeEdge="left"
          width={rightWidth}
        >
          <PreviewPane selectedDocumentId={selectedDocumentId} />
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
