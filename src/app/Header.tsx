import { useEffect, useRef, useState } from "react";
import { IconButton } from "../components/IconButton";
import type { Locale } from "./translations";

interface HeaderProps {
  compact: boolean;
  locale: Locale;
  tagline: string;
  onAddFolder?: () => void;
  onHome: () => void;
  onLocaleChange: (locale: Locale) => void;
  onSettings?: () => void;
  onStatistics?: () => void;
}

const iconProps = {
  fill: "none",
  stroke: "currentColor",
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  strokeWidth: 1.8,
  viewBox: "0 0 24 24",
};

function StatisticsIcon() {
  return (
    <svg {...iconProps}>
      <path d="M4 20V10m6 10V4m6 16v-7m4 7H2" />
    </svg>
  );
}

function AddFolderIcon() {
  return (
    <svg {...iconProps}>
      <path d="M3 7h7l2 2h9v10H3z" />
      <path d="M15 12v5m-2.5-2.5h5" />
    </svg>
  );
}

function SettingsIcon() {
  return (
    <svg {...iconProps}>
      <circle cx="12" cy="12" r="3" />
      <path d="M19 13.5v-3l-2-.7-.6-1.4.9-1.9-2.1-2.1-1.9.9-1.4-.6-.7-2h-3l-.7 2-1.4.6-1.9-.9-2.1 2.1.9 1.9-.6 1.4-2 .7v3l2 .7.6 1.4-.9 1.9 2.1 2.1 1.9-.9 1.4.6.7 2h3l.7-2 1.4-.6 1.9.9 2.1-2.1-.9-1.9.6-1.4z" />
    </svg>
  );
}

function LanguageControl({
  locale,
  onLocaleChange,
  compact = false,
}: {
  locale: Locale;
  onLocaleChange: (locale: Locale) => void;
  compact?: boolean;
}) {
  return (
    <div className={`language-control${compact ? " language-control--compact" : ""}`}>
      <label htmlFor={compact ? "language-compact" : "language"}>Language</label>
      <select
        id={compact ? "language-compact" : "language"}
        aria-label="Language"
        value={locale}
        onChange={(event) => onLocaleChange(event.target.value as Locale)}
      >
        <option value="ko">한국어</option>
        <option value="en">English</option>
      </select>
    </div>
  );
}

export function Header({
  compact,
  locale,
  tagline,
  onAddFolder,
  onHome,
  onLocaleChange,
  onSettings,
  onStatistics,
}: HeaderProps) {
  const [overflowOpen, setOverflowOpen] = useState(false);
  const overflowRoot = useRef<HTMLDivElement>(null);
  const moreTrigger = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!compact) setOverflowOpen(false);
  }, [compact]);

  useEffect(() => {
    if (!overflowOpen) return;

    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      setOverflowOpen(false);
      moreTrigger.current?.focus();
    };
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (
        event.target instanceof Node &&
        !overflowRoot.current?.contains(event.target)
      ) {
        setOverflowOpen(false);
      }
    };

    document.addEventListener("keydown", closeOnEscape);
    document.addEventListener("pointerdown", closeOnOutsidePointer);
    return () => {
      document.removeEventListener("keydown", closeOnEscape);
      document.removeEventListener("pointerdown", closeOnOutsidePointer);
    };
  }, [overflowOpen]);

  const runMenuAction = (action: (() => void) | undefined) => {
    if (!action) return;
    action();
    setOverflowOpen(false);
  };

  return (
    <header className={`app-header${compact ? " app-header--compact" : ""}`}>
      <div className="brand">
        <h1>EveryFile</h1>
        {!compact && <p>{tagline}</p>}
      </div>
      <nav className="header-actions" aria-label="주요 메뉴 / Main menu">
        <IconButton
          label="홈 / Home"
          onClick={onHome}
          icon={
            <svg {...iconProps}>
              <path d="m3 11 9-8 9 8" />
              <path d="M5 10v10h14V10M9 20v-6h6v6" />
            </svg>
          }
        />
        {compact ? (
          <div ref={overflowRoot} className="header-overflow">
            <IconButton
              ref={moreTrigger}
              label="더보기 / More"
              aria-expanded={overflowOpen}
              aria-controls="header-overflow-menu"
              onClick={() => setOverflowOpen((open) => !open)}
              icon={
                <svg {...iconProps}>
                  <circle cx="5" cy="12" r="1" fill="currentColor" />
                  <circle cx="12" cy="12" r="1" fill="currentColor" />
                  <circle cx="19" cy="12" r="1" fill="currentColor" />
                </svg>
              }
            />
            {overflowOpen && (
              <div
                id="header-overflow-menu"
                className="header-overflow-menu"
                role="group"
                aria-label="추가 메뉴 / More actions"
              >
                <IconButton
                  label="통계 / Statistics"
                  disabled={!onStatistics}
                  onClick={() => runMenuAction(onStatistics)}
                  icon={<StatisticsIcon />}
                />
                <IconButton
                  label="폴더 추가 / Add folder"
                  disabled={!onAddFolder}
                  onClick={() => runMenuAction(onAddFolder)}
                  tone="accent"
                  icon={<AddFolderIcon />}
                />
                <IconButton
                  label="설정 / Settings"
                  disabled={!onSettings}
                  onClick={() => runMenuAction(onSettings)}
                  icon={<SettingsIcon />}
                />
                <LanguageControl
                  compact
                  locale={locale}
                  onLocaleChange={onLocaleChange}
                />
              </div>
            )}
          </div>
        ) : (
          <>
            <IconButton
              label="통계 / Statistics"
              disabled={!onStatistics}
              onClick={onStatistics}
              icon={<StatisticsIcon />}
            />
            <IconButton
              label="폴더 추가 / Add folder"
              disabled={!onAddFolder}
              onClick={onAddFolder}
              tone="accent"
              icon={<AddFolderIcon />}
            />
            <IconButton
              label="설정 / Settings"
              disabled={!onSettings}
              onClick={onSettings}
              icon={<SettingsIcon />}
            />
            <LanguageControl locale={locale} onLocaleChange={onLocaleChange} />
          </>
        )}
      </nav>
    </header>
  );
}
