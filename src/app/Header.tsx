import type { Locale } from "./translations";
import { IconButton } from "../components/IconButton";

interface HeaderProps {
  locale: Locale;
  tagline: string;
  onLocaleChange: (locale: Locale) => void;
  onHome: () => void;
}

const iconProps = {
  fill: "none",
  stroke: "currentColor",
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  strokeWidth: 1.8,
  viewBox: "0 0 24 24",
};

export function Header({
  locale,
  tagline,
  onLocaleChange,
  onHome,
}: HeaderProps) {
  return (
    <header className="app-header">
      <div className="brand">
        <h1>EveryFile</h1>
        <p>{tagline}</p>
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
        <IconButton
          label="통계 / Statistics"
          icon={
            <svg {...iconProps}>
              <path d="M4 20V10m6 10V4m6 16v-7m4 7H2" />
            </svg>
          }
        />
        <IconButton
          label="폴더 추가 / Add folder"
          tone="accent"
          icon={
            <svg {...iconProps}>
              <path d="M3 7h7l2 2h9v10H3z" />
              <path d="M15 12v5m-2.5-2.5h5" />
            </svg>
          }
        />
        <IconButton
          label="설정 / Settings"
          icon={
            <svg {...iconProps}>
              <circle cx="12" cy="12" r="3" />
              <path d="M19 13.5v-3l-2-.7-.6-1.4.9-1.9-2.1-2.1-1.9.9-1.4-.6-.7-2h-3l-.7 2-1.4.6-1.9-.9-2.1 2.1.9 1.9-.6 1.4-2 .7v3l2 .7.6 1.4-.9 1.9 2.1 2.1 1.9-.9 1.4.6.7 2h3l.7-2 1.4-.6 1.9.9 2.1-2.1-.9-1.9.6-1.4z" />
            </svg>
          }
        />
        <div className="language-control">
          <label htmlFor="language">Language</label>
          <select
            id="language"
            aria-label="Language"
            value={locale}
            onChange={(event) => onLocaleChange(event.target.value as Locale)}
          >
            <option value="ko">한국어</option>
            <option value="en">English</option>
          </select>
        </div>
      </nav>
    </header>
  );
}
