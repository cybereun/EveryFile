import {
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
  useRef,
} from "react";

interface ResizablePaneProps {
  children: ReactNode;
  className?: string;
  hidden?: boolean;
  label: string;
  maxWidth: number;
  minWidth: number;
  onWidthChange: (width: number) => void;
  resizeEdge: "left" | "right";
  width: number;
}

const KEYBOARD_STEP = 16;

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, Math.round(value)));
}

export function ResizablePane({
  children,
  className = "",
  hidden = false,
  label,
  maxWidth,
  minWidth,
  onWidthChange,
  resizeEdge,
  width,
}: ResizablePaneProps) {
  const dragStart = useRef<{ pointerX: number; width: number } | null>(null);

  const resizeFromPointer = (event: PointerEvent<HTMLDivElement>) => {
    if (!dragStart.current) return;
    const movement = event.clientX - dragStart.current.pointerX;
    const direction = resizeEdge === "right" ? 1 : -1;
    onWidthChange(
      clamp(dragStart.current.width + movement * direction, minWidth, maxWidth),
    );
  };

  const finishPointerResize = (event: PointerEvent<HTMLDivElement>) => {
    dragStart.current = null;
    if (event.currentTarget.hasPointerCapture?.(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };

  const resizeFromKeyboard = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Home") {
      event.preventDefault();
      onWidthChange(minWidth);
      return;
    }
    if (event.key === "End") {
      event.preventDefault();
      onWidthChange(maxWidth);
      return;
    }
    if (!["ArrowLeft", "ArrowRight"].includes(event.key)) return;

    event.preventDefault();
    const arrowDirection = event.key === "ArrowRight" ? 1 : -1;
    const edgeDirection = resizeEdge === "right" ? 1 : -1;
    onWidthChange(
      clamp(width + KEYBOARD_STEP * arrowDirection * edgeDirection, minWidth, maxWidth),
    );
  };

  const separator = (
    <div
      className="pane-separator"
      role="separator"
      aria-label={label}
      aria-orientation="vertical"
      aria-valuemin={minWidth}
      aria-valuemax={maxWidth}
      aria-valuenow={width}
      tabIndex={0}
      onKeyDown={resizeFromKeyboard}
      onPointerDown={(event) => {
        dragStart.current = { pointerX: event.clientX, width };
        event.currentTarget.setPointerCapture?.(event.pointerId);
      }}
      onPointerMove={resizeFromPointer}
      onPointerUp={finishPointerResize}
      onPointerCancel={finishPointerResize}
    />
  );

  return (
    <div
      className={`resizable-pane ${className}`.trim()}
      style={{ width: `${width}px` }}
      hidden={hidden}
    >
      {resizeEdge === "left" && separator}
      <div className="resizable-pane__content">{children}</div>
      {resizeEdge === "right" && separator}
    </div>
  );
}
