import { useEffect, useRef, useState } from "react";
import { IconButton } from "../components/IconButton";
import { BrandMark } from "../components/BrandMark";
import { useI18n } from "./translations";

interface HeaderProps {
  compact: boolean;
  tagline: string;
  onAddFolder?: () => void;
  leftPanelOpen?: boolean;
  rightPanelOpen?: boolean;
  onToggleLeftPanel?: () => void;
  onToggleRightPanel?: () => void;
  onHome: () => void;
  onSettings?: () => void;
  onStatistics?: () => void;
}

const premiumIconProps = {
  fill: "none",
  viewBox: "0 0 24 24",
  xmlns: "http://www.w3.org/2000/svg",
};

function HomeIcon() {
  return (
    <svg {...premiumIconProps}>
      <defs>
        <linearGradient id="header-home-clay" x1="4" y1="3" x2="19" y2="20" gradientUnits="userSpaceOnUse">
          <stop stopColor="#E7CDB0" />
          <stop offset="1" stopColor="#A47F5E" />
        </linearGradient>
      </defs>
      <path d="m3.5 10.8 8.5-7 8.5 7v7.7a1.6 1.6 0 0 1-1.6 1.6H5.1a1.6 1.6 0 0 1-1.6-1.6v-7.7Z" fill="url(#header-home-clay)" stroke="#5D4633" strokeWidth="1.35" strokeLinejoin="round" />
      <path d="M9 20.1v-6.5h6v6.5" fill="#FFF8EE" stroke="#6E513A" strokeWidth="1.25" strokeLinejoin="round" />
      <path d="m5.2 10.2 6.8-5.5 6.8 5.5" stroke="#FFF8EE" strokeWidth="1.1" strokeLinecap="round" opacity="0.75" />
    </svg>
  );
}

function StatisticsIcon() {
  return (
    <svg {...premiumIconProps}>
      <defs>
        <linearGradient id="header-stats-clay" x1="4" y1="6" x2="20" y2="20" gradientUnits="userSpaceOnUse">
          <stop stopColor="#E3C8A8" />
          <stop offset="1" stopColor="#9D7958" />
        </linearGradient>
      </defs>
      <path d="M3.5 20h17" stroke="#5D4633" strokeWidth="1.35" strokeLinecap="round" />
      <rect x="4.5" y="13" width="3.2" height="6" rx="1.1" fill="url(#header-stats-clay)" stroke="#6B4F38" strokeWidth="0.8" />
      <rect x="10.4" y="9" width="3.2" height="10" rx="1.1" fill="url(#header-stats-clay)" stroke="#6B4F38" strokeWidth="0.8" />
      <rect x="16.3" y="5" width="3.2" height="14" rx="1.1" fill="#D87350" stroke="#98452F" strokeWidth="0.8" />
      <path d="m5.4 11.3 4.1-3 3.2 1.2 4.2-4" stroke="#C45F43" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />
      <circle cx="5.4" cy="11.3" r="1" fill="#FFF7EC" stroke="#8C6246" strokeWidth="0.8" />
      <circle cx="9.5" cy="8.3" r="1" fill="#FFF7EC" stroke="#8C6246" strokeWidth="0.8" />
      <circle cx="12.7" cy="9.5" r="1" fill="#FFF7EC" stroke="#8C6246" strokeWidth="0.8" />
      <circle cx="16.9" cy="5.5" r="1" fill="#F28D68" stroke="#98452F" strokeWidth="0.8" />
    </svg>
  );
}

function AddFolderIcon() {
  return (
    <svg {...premiumIconProps}>
      <path d="M3.2 7.6c0-1 .8-1.8 1.8-1.8h5.1l1.8 2h7.1c1 0 1.8.8 1.8 1.8v8.7c0 1-.8 1.8-1.8 1.8H5c-1 0-1.8-.8-1.8-1.8V7.6Z" fill="#E4C9A9" stroke="#694C36" strokeWidth="1.2" strokeLinejoin="round" />
      <path d="M4.1 9.2h16.2" stroke="#FFF7EC" strokeWidth="1.1" strokeLinecap="round" opacity="0.75" />
      <circle cx="17.1" cy="16.7" r="3.8" fill="#D87350" stroke="#98452F" strokeWidth="0.9" />
      <path d="M17.1 14.8v3.8m-1.9-1.9H19" stroke="#FFF8EE" strokeWidth="1.15" strokeLinecap="round" />
    </svg>
  );
}

function SettingsIcon() {
  return (
    <svg {...premiumIconProps}>
      <path d="M19.3 13.5a7.8 7.8 0 0 0 0-3l1.6-1.2-1.6-2.7-1.9.8a7.7 7.7 0 0 0-2.6-1.5L14.5 4h-3.1l-.3 1.9a7.7 7.7 0 0 0-2.6 1.5l-1.9-.8L5 9.3l1.6 1.2a7.8 7.8 0 0 0 0 3L5 14.7l1.6 2.7 1.9-.8a7.7 7.7 0 0 0 2.6 1.5l.3 1.9h3.1l.3-1.9a7.7 7.7 0 0 0 2.6-1.5l1.9.8 1.6-2.7-1.6-1.2Z" fill="#B79A7B" stroke="#5D4633" strokeWidth="1.05" strokeLinejoin="round" />
      <circle cx="13" cy="12" r="3.1" fill="#FFF8EE" stroke="#6E513A" strokeWidth="1.1" />
      <circle cx="13" cy="12" r="1.15" fill="#D87350" />
    </svg>
  );
}

function PanelIcon({ side }: { side: "left" | "right" }) {
  return (
    <svg {...premiumIconProps}>
      <rect x="3.1" y="4" width="17.8" height="16" rx="3" fill="#E9D8C2" stroke="#654A35" strokeWidth="1.15" />
      <path d={side === "left" ? "M9.2 4.8v14.4" : "M14.8 4.8v14.4"} stroke="#9A7656" strokeWidth="1.05" strokeLinecap="round" />
      <path d={side === "left" ? "M4.5 5.7h3.3v12.6H4.5z" : "M16.2 5.7h3.3v12.6h-3.3z"} fill="#D87350" opacity="0.78" />
      <path d="M5.1 5.7h13.8" stroke="#FFF8EE" strokeWidth="0.9" strokeLinecap="round" opacity="0.7" />
    </svg>
  );
}

function MoreIcon() {
  return (
    <svg {...premiumIconProps}>
      <circle cx="5.4" cy="12" r="1.45" fill="#B79A7B" stroke="#644A36" strokeWidth="0.7" />
      <circle cx="12" cy="12" r="1.45" fill="#D87350" stroke="#98452F" strokeWidth="0.7" />
      <circle cx="18.6" cy="12" r="1.45" fill="#B79A7B" stroke="#644A36" strokeWidth="0.7" />
    </svg>
  );
}

export function Header({
  compact,
  tagline,
  onAddFolder,
  leftPanelOpen = true,
  rightPanelOpen = true,
  onToggleLeftPanel = () => undefined,
  onToggleRightPanel = () => undefined,
  onHome,
  onSettings,
  onStatistics,
}: HeaderProps) {
  const { t } = useI18n();
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
        <BrandMark />
        <h1>EveryFile</h1>
        {!compact && <p>{tagline}</p>}
      </div>
      <nav className="header-actions" aria-label={t("주요 메뉴 / Main menu")}>
        <IconButton
          label={t("홈 / Home")}
          onClick={onHome}
          icon={
            <HomeIcon />
          }
        />
        <IconButton
          label={t("폴더 추가 / Add folder")}
          disabled={!onAddFolder}
          onClick={onAddFolder}
          icon={<AddFolderIcon />}
        />
        {compact ? (
          <div ref={overflowRoot} className="header-overflow">
            <IconButton
              ref={moreTrigger}
              label={t("더보기 / More")}
              aria-expanded={overflowOpen}
              aria-controls="header-overflow-menu"
              onClick={() => setOverflowOpen((open) => !open)}
              icon={<MoreIcon />}
            />
            {overflowOpen && (
              <div
                id="header-overflow-menu"
                className="header-overflow-menu"
                role="group"
                aria-label={t("추가 메뉴 / More actions")}
              >
                <IconButton
                  label={`${t(leftPanelOpen ? "왼쪽 패널 닫기" : "왼쪽 패널 열기")} / Toggle left panel`}
                  aria-pressed={leftPanelOpen}
                  onClick={() => runMenuAction(onToggleLeftPanel)}
                  icon={<PanelIcon side="left" />}
                />
                <IconButton
                  label={`${t(rightPanelOpen ? "오른쪽 패널 닫기" : "오른쪽 패널 열기")} / Toggle right panel`}
                  aria-pressed={rightPanelOpen}
                  onClick={() => runMenuAction(onToggleRightPanel)}
                  icon={<PanelIcon side="right" />}
                />
                <IconButton
                  label={t("통계 / Statistics")}
                  disabled={!onStatistics}
                  onClick={() => runMenuAction(onStatistics)}
                  icon={<StatisticsIcon />}
                />
                <IconButton
                  label={t("설정 / Settings")}
                  disabled={!onSettings}
                  onClick={() => runMenuAction(onSettings)}
                  icon={<SettingsIcon />}
                />
              </div>
            )}
          </div>
        ) : (
          <>
            <IconButton
              label={`${t(leftPanelOpen ? "왼쪽 패널 닫기" : "왼쪽 패널 열기")} / Toggle left panel`}
              aria-pressed={leftPanelOpen}
              onClick={onToggleLeftPanel}
              icon={<PanelIcon side="left" />}
            />
            <IconButton
              label={`${t(rightPanelOpen ? "오른쪽 패널 닫기" : "오른쪽 패널 열기")} / Toggle right panel`}
              aria-pressed={rightPanelOpen}
              onClick={onToggleRightPanel}
              icon={<PanelIcon side="right" />}
            />
            <IconButton
              label={t("통계 / Statistics")}
              disabled={!onStatistics}
              onClick={onStatistics}
              icon={<StatisticsIcon />}
            />
            <IconButton
              label={t("설정 / Settings")}
              disabled={!onSettings}
              onClick={onSettings}
              icon={<SettingsIcon />}
            />
          </>
        )}
      </nav>
    </header>
  );
}
