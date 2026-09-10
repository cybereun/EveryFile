import {
  type ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { ResizablePane } from "../components/ResizablePane";
import {
  CommandStatus,
  type CommandStatusMessage,
} from "../components/CommandStatus";
import { IndexStatusController } from "../features/folders/IndexStatus";
import { PreviewPanel } from "../features/preview/PreviewPanel";
import { SearchWorkspace } from "../features/search/SearchWorkspace";
import { SettingsDialog, type SettingsTab } from "../features/settings/SettingsDialog";
import { StatisticsDialog } from "../features/statistics/StatisticsDialog";
import { UpdateDialog } from "../features/update/UpdateDialog";
import type {
  AppSettings,
  BookmarkSummary,
  FolderRecord,
  SearchHistoryRecord,
  StatisticsSearchFilter,
} from "../lib/types";
import { checkForUpdate, type AvailableUpdate } from "../lib/updater";
import {
  clearSearchHistory,
  getSettings,
  listBookmarks,
  listSearchHistory,
} from "../lib/ipc";
import "../styles/app.css";
import "../styles/refresh.css";
import { Header } from "./Header";
import {
  createTranslator,
  defaultLocale,
  I18nProvider,
  type Locale,
  useI18n,
} from "./translations";

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
const APP_VERSION = "v1.1.2";
const UPDATE_INTERVAL_MS = 6 * 60 * 60 * 1000;

function displayPath(path: string) {
  return path
    .replace(/^\\\\\?\\UNC\\/i, "\\\\")
    .replace(/^\\\\\?\\/, "");
}

function relativeSearchTime(value: string, locale: Locale = defaultLocale) {
  const timestamp = new Date(value).getTime();
  if (!Number.isFinite(timestamp)) return locale === "en" ? "Unknown date" : "날짜 없음";
  const elapsed = Math.max(0, Date.now() - timestamp);
  const formatter = new Intl.RelativeTimeFormat(locale === "en" ? "en-US" : "ko-KR", { numeric: "auto" });
  if (elapsed < 60_000) return formatter.format(0, "second");
  if (elapsed < 60 * 60_000) return formatter.format(-Math.floor(elapsed / 60_000), "minute");
  if (elapsed < 24 * 60 * 60_000) return formatter.format(-Math.floor(elapsed / (60 * 60_000)), "hour");
  return formatter.format(-Math.floor(elapsed / (24 * 60 * 60_000)), "day");
}

interface SmartFolderRecord {
  id: string;
  name: string;
  query: string;
}

const SMART_FOLDERS_KEY = "everyfile.smart-folders";

function readSmartFolders() {
  try {
    const stored = JSON.parse(window.localStorage.getItem(SMART_FOLDERS_KEY) ?? "[]");
    if (!Array.isArray(stored)) return [];
    return stored.filter(
      (item): item is SmartFolderRecord =>
        Boolean(item) &&
        typeof item.id === "string" &&
        typeof item.name === "string" &&
        typeof item.query === "string",
    );
  } catch {
    return [];
  }
}

export interface AppProps {
  folders?: FolderRecord[];
  indexedDocumentCount?: number;
  queueState?: "idle" | "indexing" | "paused" | "error";
  reportJobIds?: ReadonlySet<string>;
  selectedDocumentId?: string | null;
  onAddFolder?: () => void;
  onRemoveFolder?: (folderId: string) => void;
  onOpenFolder?: (folderId: string) => void;
  onReindexFolder?: (folderId: string) => void;
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

function SidebarTrashIcon() {
  return (
    <svg
      className="sidebar-trash-icon"
      viewBox="0 0 20 20"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      focusable="false"
      aria-hidden="true"
    >
      <path
        d="M5.1 6.2h9.8l-.55 9.05a1.55 1.55 0 0 1-1.55 1.45H7.2a1.55 1.55 0 0 1-1.55-1.45L5.1 6.2Z"
        fill="currentColor"
        fillOpacity="0.18"
        stroke="currentColor"
        strokeWidth="1.15"
        strokeLinejoin="round"
      />
      <path
        d="M3.8 5.35h12.4M7.5 3.55h5a.9.9 0 0 1 .9.9v.9H6.6v-.9a.9.9 0 0 1 .9-.9Z"
        stroke="currentColor"
        strokeWidth="1.15"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M8.1 8.3v5.35M11.9 8.3v5.35"
        stroke="currentColor"
        strokeWidth="0.95"
        strokeLinecap="round"
        opacity="0.8"
      />
      <path d="M15.8 3.1h1.1" stroke="var(--color-accent)" strokeWidth="1.15" strokeLinecap="round" />
    </svg>
  );
}

function FolderPane({
  folders,
  onAddFolder,
  onRemoveFolder,
  onOpenFolder,
  onReindexFolder,
  onSearchHistory,
  onBookmarkSelect,
  historyRefreshToken = 0,
  bookmarkRefreshToken = 0,
}: {
  folders: FolderRecord[];
  onAddFolder?: () => void;
  onRemoveFolder?: (folderId: string) => void;
  onOpenFolder?: (folderId: string) => void;
  onReindexFolder?: (folderId: string) => void;
  onSearchHistory?: (query: string) => void;
  onBookmarkSelect?: (documentId: string) => void;
  historyRefreshToken?: number;
  bookmarkRefreshToken?: number;
}) {
  const { locale, t } = useI18n();
  const [history, setHistory] = useState<SearchHistoryRecord[]>([]);
  const [historyError, setHistoryError] = useState("");
  const [bookmarks, setBookmarks] = useState<BookmarkSummary[]>([]);
  const [smartFolders, setSmartFolders] = useState<SmartFolderRecord[]>(readSmartFolders);
  const [folderMenu, setFolderMenu] = useState<string | null>(null);
  const [removeCandidate, setRemoveCandidate] = useState<FolderRecord | null>(null);
  const [favoriteFolders, setFavoriteFolders] = useState<Set<string>>(() => {
    try {
      const stored = JSON.parse(window.localStorage.getItem("everyfile.favorite-folders") ?? "[]");
      return new Set(Array.isArray(stored) ? stored.filter((id): id is string => typeof id === "string") : []);
    } catch {
      return new Set();
    }
  });

  useEffect(() => {
    let active = true;
    void listSearchHistory(3, 0)
      .then((records) => { if (active) setHistory(records); })
      .catch(() => undefined);
    return () => { active = false; };
  }, [historyRefreshToken]);

  useEffect(() => {
    let active = true;
    void listBookmarks()
      .then((records) => { if (active) setBookmarks(records); })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [bookmarkRefreshToken]);

  const clearRecentHistory = async () => {
    const previous = history;
    setHistoryError("");
    // Reflect the action immediately while the encrypted database operation
    // completes. If it fails, restore the current records below.
    setHistory([]);
    try {
      const deleted = await clearSearchHistory();
      if (deleted === 0 && previous.length > 0) {
        const current = await listSearchHistory(3, 0);
        if (current.length > 0) {
          setHistory(current);
          setHistoryError(t("검색 히스토리를 삭제하지 못했습니다."));
        }
      }
    } catch {
      const current = await listSearchHistory(3, 0).catch(() => previous);
      setHistory(current);
      setHistoryError(t("검색 히스토리를 삭제하지 못했습니다."));
    }
  };

  const addSmartFolder = () => {
    try {
      const preset = JSON.parse(
        window.localStorage.getItem("everyfile.search.current") ??
          window.localStorage.getItem("everyfile.search.preset") ??
          "null",
      ) as { query?: unknown } | null;
      const query = typeof preset?.query === "string" ? preset.query.trim() : "";
      if (!query) return;
      const existing = smartFolders.find((item) => item.query === query);
      if (existing) return;
      const next = [
        ...smartFolders,
        { id: `smart-${Date.now()}`, name: query, query },
      ];
      setSmartFolders(next);
      window.localStorage.setItem(SMART_FOLDERS_KEY, JSON.stringify(next));
    } catch {
      // Keep the sidebar usable when browser storage is unavailable.
    }
  };

  const removeSmartFolder = (id: string) => {
    const next = smartFolders.filter((item) => item.id !== id);
    setSmartFolders(next);
    try {
      window.localStorage.setItem(SMART_FOLDERS_KEY, JSON.stringify(next));
    } catch {
      // Keep the in-memory list usable when browser storage is unavailable.
    }
  };

  useEffect(() => {
    if (!folderMenu) return;
    const close = (event: MouseEvent) => {
      if (!(event.target as Element | null)?.closest(".folder-item__menu-wrap")) setFolderMenu(null);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setFolderMenu(null);
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", escape);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", escape);
    };
  }, [folderMenu]);

  const toggleFavorite = (folderId: string) => {
    setFavoriteFolders((current) => {
      const next = new Set(current);
      if (next.has(folderId)) next.delete(folderId);
      else next.add(folderId);
      try {
        window.localStorage.setItem("everyfile.favorite-folders", JSON.stringify([...next]));
      } catch {
        // Keep the session preference when local storage is unavailable.
      }
      return next;
    });
    setFolderMenu(null);
  };

  const section = (
    title: string,
    count: number,
    children: ReactNode,
    action?: ReactNode,
  ) => (
    <details className="sidebar-section" open>
      <summary>
        <span>{title}</span>
        <small>({count.toLocaleString()})</small>
        {action}
      </summary>
      <div className="sidebar-section__body">{children}</div>
    </details>
  );

  return (
    <aside className="folder-pane" aria-label={locale === "en" ? t("색인된 폴더") : "등록 폴더 / Indexed folders"}>
      <div className="sidebar-scroll">
        {section(
          t("색인된 폴더"),
          folders.length,
          folders.length === 0 ? (
            <p className="sidebar-empty">{t("선택한 폴더만 이 PC에서 색인합니다.")}</p>
          ) : (
            <div className="folder-list">
              {folders.map((folder) => (
                <div className="folder-item" key={folder.id}>
                  <span className="sidebar-item__icon" aria-hidden="true">▰</span>
                  <span className="sidebar-item__label" title={displayPath(folder.canonicalPath)}>{folder.displayName}</span>
                  <span className="folder-count">{folder.documentCount.toLocaleString()}</span>
                  <div className="folder-item__menu-wrap">
                    <button
                      type="button"
                      className="folder-menu-trigger"
                      aria-label={`${folder.displayName} ${t("폴더")} 메뉴`}
                      aria-expanded={folderMenu === folder.id}
                      aria-haspopup="menu"
                      onClick={() => setFolderMenu((current) => current === folder.id ? null : folder.id)}
                    >⋯</button>
                    {folderMenu === folder.id && (
                      <div className="folder-context-menu" role="menu">
                        <button role="menuitem" type="button" onClick={() => toggleFavorite(folder.id)}>
                          <span aria-hidden="true">☆</span>{favoriteFolders.has(folder.id) ? t("즐겨찾기 해제") : t("즐겨찾기 추가")}
                        </button>
                        <button role="menuitem" type="button" onClick={() => { setFolderMenu(null); onOpenFolder?.(folder.id); }} disabled={!onOpenFolder}>
                          <span aria-hidden="true">▱</span>{t("탐색기에서 열기")}
                        </button>
                        <button role="menuitem" type="button" onClick={() => { setFolderMenu(null); onReindexFolder?.(folder.id); }} disabled={!onReindexFolder}>
                          <span aria-hidden="true">↻</span>{t("재인덱싱")}
                        </button>
                        <div className="folder-context-menu__separator" role="separator" />
                        <button className="is-danger" role="menuitem" type="button" onClick={() => { setFolderMenu(null); setRemoveCandidate(folder); }} disabled={!onRemoveFolder}>
                          <span aria-hidden="true">♲</span>{t("폴더 제거")}
                        </button>
                      </div>
                    )}
                  </div>
                </div>
              ))}
            </div>
          ),
          <button type="button" className="sidebar-section__action" aria-label={t("폴더 추가")} onClick={(event) => { event.preventDefault(); onAddFolder?.(); }} disabled={!onAddFolder}>＋</button>,
        )}
        {section(
          t("스마트 폴더"),
          smartFolders.length,
          smartFolders.length === 0 ? (
            <p className="sidebar-helper"><span aria-hidden="true">⌕</span> {t("자주 쓰는 검색 조건을 저장하면 여기서 한 번에 다시 실행할 수 있어요.")}</p>
          ) : (
            <ul className="sidebar-link-list">
              {smartFolders.map((item) => (
                <li className="sidebar-link-row" key={item.id}>
                  <button className="sidebar-link-list__item" type="button" title={item.query} onClick={() => onSearchHistory?.(item.query)}>
                    <span aria-hidden="true">⌕</span><span>{item.name}</span>
                  </button>
                  <button
                    type="button"
                    className="sidebar-link-list__remove"
                    aria-label={`${item.name} ${t("스마트 폴더 삭제")}`}
                    title={t("스마트 폴더 삭제")}
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      removeSmartFolder(item.id);
                    }}
                  >×</button>
                </li>
              ))}
            </ul>
          ),
          <button type="button" className="sidebar-section__action" aria-label={t("스마트 폴더 추가")} onClick={(event) => { event.preventDefault(); event.stopPropagation(); addSmartFolder(); }}>＋</button>,
        )}
        {section(
          t("최근 검색"),
          history.length,
          history.length === 0 ? (
            <p className="sidebar-empty">{t("최근 검색이 없습니다.")}</p>
          ) : (
            <ul className="sidebar-link-list">
              {history.map((item) => (
                <li key={item.id}>
                  <button type="button" title={item.query} onClick={() => onSearchHistory?.(item.query)}><span aria-hidden="true">⌕</span><span>{item.query}</span><time dateTime={item.searchedAt}>{relativeSearchTime(item.searchedAt, locale)}</time></button>
                </li>
              ))}
            </ul>
          ),
          <button type="button" className="sidebar-section__action" aria-label={t("최근 검색 삭제")} onClick={(event) => { event.preventDefault(); event.stopPropagation(); void clearRecentHistory(); }}><SidebarTrashIcon /></button>,
        )}
        {section(
          t("북마크"),
          bookmarks.length,
          bookmarks.length === 0 ? (
            <p className="sidebar-empty">{t("북마크가 없습니다.")}</p>
          ) : (
            <ul className="sidebar-link-list">
              {bookmarks.map((bookmark) => (
                <li key={bookmark.documentId}>
                  <button
                    type="button"
                    title={bookmark.path}
                    onClick={() => onBookmarkSelect?.(bookmark.documentId)}
                  >
                    <span aria-hidden="true">☆</span>
                    <span>{bookmark.fileName}</span>
                  </button>
                </li>
              ))}
            </ul>
          ),
        )}
      </div>
      {historyError && <p className="sidebar-error" role="alert">{historyError}</p>}
      <div className="sidebar-credit">
        <span>© 2026 Lebi_Cybereun</span>
        <a href="mailto:cybereunny@gmail.com">cybereunny@gmail.com</a>
      </div>
      {removeCandidate && (
        <div className="confirmation-backdrop" role="presentation">
          <section className="folder-remove-dialog" role="alertdialog" aria-modal="true" aria-labelledby="folder-remove-title">
            <h2 id="folder-remove-title">{t("색인 폴더를 제거할까요?")}</h2>
            <p><strong>{removeCandidate.displayName}</strong> {t("폴더의 색인 정보만 제거하며 원본 파일은 삭제하지 않습니다.")}</p>
            <div>
              <button type="button" onClick={() => setRemoveCandidate(null)}>{t("취소")}</button>
              <button className="is-danger" type="button" onClick={() => { onRemoveFolder?.(removeCandidate.id); setRemoveCandidate(null); }}>{t("폴더 제거")}</button>
            </div>
          </section>
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
  reportJobIds,
  selectedDocumentId = null,
  onAddFolder,
  onRemoveFolder,
  onOpenFolder,
  onReindexFolder,
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
  const [previewSearchQuery, setPreviewSearchQuery] = useState("");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [statisticsOpen, setStatisticsOpen] = useState(false);
  const [statisticsFilter, setStatisticsFilter] =
    useState<StatisticsSearchFilter | null>(null);
  const [historyQuery, setHistoryQuery] = useState<string | null>(null);
  const [historyRequest, setHistoryRequest] = useState(0);
  const [historyRefreshToken, setHistoryRefreshToken] = useState(0);
  const [bookmarkRefreshToken, setBookmarkRefreshToken] = useState(0);
  const [interactionNotice, setInteractionNotice] =
    useState<CommandStatusMessage | null>(null);
  const [homeRequest, setHomeRequest] = useState(0);
  const [focusSearchRequest, setFocusSearchRequest] = useState(0);
  const [settingsInitialTab, setSettingsInitialTab] = useState<SettingsTab>("general");
  const [appSettings, setAppSettings] = useState<AppSettings | null>(null);
  const [availableUpdate, setAvailableUpdate] = useState<AvailableUpdate | null>(null);
  const handleHistoryChanged = useCallback(() => {
    setHistoryRefreshToken((token) => token + 1);
  }, []);
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
  const t = createTranslator(locale);
  const tagline = t("파일을 찾는 가장 빠른 방법");
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

  const runUpdateCheck = useCallback(async () => {
    const update = await checkForUpdate();
    if (update) setAvailableUpdate(update);
    return update;
  }, []);

  useEffect(() => {
    if (!appSettings || appSettings.autoUpdateEnabled === false) return;
    let active = true;
    const checkInBackground = async () => {
      try {
        const update = await checkForUpdate();
        if (active && update) setAvailableUpdate(update);
      } catch {
        // Automatic checks are deliberately quiet; the diagnostics tab reports
        // errors when the user explicitly requests a check.
      }
    };
    void checkInBackground();
    const timer = window.setInterval(() => void checkInBackground(), UPDATE_INTERVAL_MS);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [appSettings?.autoUpdateEnabled, appSettings]);

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
        setFocusSearchRequest((request) => request + 1);
        searchInput.current?.focus();
      }
    };

    window.addEventListener("keydown", handleShortcut);
    return () => window.removeEventListener("keydown", handleShortcut);
  }, []);

  return (
    <I18nProvider locale={locale}>
      <div className="app-shell">
      <Header
        compact={compactHeader}
        tagline={tagline}
        onAddFolder={onAddFolder}
        leftPanelOpen={leftPanelOpen}
        rightPanelOpen={rightPanelOpen}
        onToggleLeftPanel={() => setLeftPanelOpen((open) => !open)}
        onToggleRightPanel={() => setRightPanelOpen((open) => !open)}
        onHome={() => {
          setLeftPanelOpen(true);
          setWorkspaceSelectedDocumentId(null);
          setPreviewSearchQuery("");
          setHistoryQuery(null);
          setInteractionNotice(null);
          setStatisticsFilter(null);
          setHomeRequest((request) => request + 1);
          setFocusSearchRequest((request) => request + 1);
          searchInput.current?.focus();
        }}
        onSettings={() => {
          onSettings?.();
          setSettingsInitialTab("general");
          setSettingsOpen(true);
        }}
        onStatistics={() => {
          onStatistics?.();
          setStatisticsOpen(true);
        }}
      />
      <div className="command-status-slot">
        <CommandStatus
          message={interactionNotice ?? commandStatus}
          onDismiss={() => {
            setInteractionNotice(null);
            onDismissCommandStatus?.();
          }}
        />
      </div>
      <main ref={workspaceRef} className="workspace">
        <ResizablePane
          className="left-pane-container"
          hidden={!leftPaneVisible}
          label={t("폴더 패널 크기 조절 / Resize folder pane")}
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
            onOpenFolder={onOpenFolder}
            onReindexFolder={onReindexFolder}
            onBookmarkSelect={(documentId) => {
              setWorkspaceSelectedDocumentId(documentId);
              setPreviewSearchQuery("");
              setInteractionNotice(null);
            }}
            historyRefreshToken={historyRefreshToken}
            bookmarkRefreshToken={bookmarkRefreshToken}
            onSearchHistory={(query) => {
              setHistoryQuery(query);
              setHistoryRequest((request) => request + 1);
              setStatisticsFilter(null);
            }}
          />
        </ResizablePane>

        <section className="center-pane" aria-label={t("검색 작업 공간 / Search workspace")}>
          <SearchWorkspace
            folders={folders}
            onAddFolder={onAddFolder}
            historyQuery={historyQuery}
            historyRequest={historyRequest}
            homeRequest={homeRequest}
            statisticsFilter={statisticsFilter}
            aiEnabled={appSettings?.aiEnabled ?? false}
            aiProvider={appSettings?.aiProvider ?? "ollama"}
            selectedDocumentId={workspaceSelectedDocumentId}
            focusSearchRequest={focusSearchRequest}
            onOpenAiSettings={() => {
              setSettingsInitialTab("ai");
              setSettingsOpen(true);
            }}
            onHistoryChanged={handleHistoryChanged}
            pageSize={appSettings?.resultPageSize}
            fileClickBehavior={appSettings?.fileClickBehavior}
            dateDisplay={appSettings?.dateDisplay}
            onSelectDocument={(documentId, query) => {
              setWorkspaceSelectedDocumentId(documentId);
              setPreviewSearchQuery(query);
              setInteractionNotice(null);
              onDocumentSelect?.(documentId);
            }}
            ref={searchInput}
          />
        </section>

        <ResizablePane
          className="preview-pane-container"
          hidden={!previewVisible}
          label={t("미리보기 패널 크기 조절 / Resize preview pane")}
          maxWidth={rightMaximumForLayout}
          minWidth={RIGHT_PANE_MIN}
          onWidthChange={setRightWidth}
          resizeEdge="left"
          width={renderedRightWidth}
        >
          <PreviewPanel
            documentId={workspaceSelectedDocumentId}
            searchQuery={previewSearchQuery}
            aiEnabled={appSettings?.aiEnabled ?? false}
            aiProvider={appSettings?.aiProvider ?? "ollama"}
            onBookmarkChanged={() => setBookmarkRefreshToken((token) => token + 1)}
          />
        </ResizablePane>
      </main>
      <SettingsDialog
        folders={folders}
        initialTab={settingsInitialTab}
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        onCheckForUpdates={runUpdateCheck}
        onSaved={(settings) => {
          setAppSettings(settings);
          if (settings.language === "ko" || settings.language === "en") {
            setLocale(settings.language);
          }
        }}
      />
      <UpdateDialog
        update={availableUpdate}
        onClose={() => setAvailableUpdate(null)}
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
        <IndexStatusController
          reportJobIds={reportJobIds}
          idleContent={
            <div className="status-summary" role="status" aria-live="polite">
              <span>{t("색인 문서")} {indexedDocumentCount.toLocaleString()}{t("개")}</span>
              <span>{t("폴더")} {folders.length.toLocaleString()}{t("개")}</span>
              <span>{t("대기열")} {t(queueLabels[queueState])}</span>
              <span className="app-version">{APP_VERSION}</span>
            </div>
          }
        />
      </footer>
      </div>
    </I18nProvider>
  );
}
