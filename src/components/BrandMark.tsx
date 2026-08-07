interface BrandMarkProps {
  className?: string;
}

export function BrandMark({ className = "" }: BrandMarkProps) {
  return (
    <img
      aria-hidden="true"
      className={`brand-mark ${className}`.trim()}
      src="/everyfile-icon.svg"
      alt=""
      draggable={false}
    />
  );
}
