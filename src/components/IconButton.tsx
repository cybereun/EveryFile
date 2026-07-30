import type { ButtonHTMLAttributes, ReactNode } from "react";

interface IconButtonProps
  extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "aria-label" | "children"> {
  label: string;
  icon: ReactNode;
  tone?: "default" | "accent";
}

export function IconButton({
  label,
  icon,
  tone = "default",
  className = "",
  ...buttonProps
}: IconButtonProps) {
  const classes = [
    "icon-button",
    tone === "accent" ? "icon-button--accent" : "",
    className,
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <button
      {...buttonProps}
      type={buttonProps.type ?? "button"}
      className={classes}
      aria-label={label}
      title={label}
    >
      <span aria-hidden="true">{icon}</span>
    </button>
  );
}
