interface BrandMarkProps {
  className?: string;
}

export function BrandMark({ className = "" }: BrandMarkProps) {
  return (
    <svg
      aria-hidden="true"
      className={`brand-mark ${className}`.trim()}
      fill="none"
      viewBox="0 0 32 32"
    >
      <rect x="2" y="2" width="28" height="28" rx="8" fill="currentColor" opacity="0.14" />
      <path d="M7.5 11h7l2.1 2.3H25v9.2H7.5z" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" />
      <circle cx="18.5" cy="16.5" r="4.3" fill="var(--color-surface)" stroke="currentColor" strokeWidth="1.8" />
      <path d="m21.6 19.6 3.2 3.2" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
    </svg>
  );
}
