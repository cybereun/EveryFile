import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";

interface PreviewToolbarProps {
  bookmark: ReactNode;
  onAddTag: () => void;
  onCopyPath: () => void;
  onCopyText: () => void;
  onFind: () => void;
  onOpen: () => void;
  onOpenLocation: () => void;
  onSaveMarkdown: () => void;
}

export function PreviewToolbar({
  bookmark,
  onAddTag,
  onCopyPath,
  onCopyText,
  onFind,
  onOpen,
  onOpenLocation,
  onSaveMarkdown,
}: PreviewToolbarProps) {
  const [moreOpen, setMoreOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!moreOpen) return;
    const close = (event: MouseEvent) => {
      if (!root.current?.contains(event.target as Node)) setMoreOpen(false);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setMoreOpen(false);
    };
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", escape);
    };
  }, [moreOpen]);

  const run = (action: () => void) => {
    setMoreOpen(false);
    action();
  };

  return (
    <div className="preview-toolbar" aria-label="미리보기 도구" ref={root}>
      <button onClick={onOpen} type="button">파일 열기</button>
      <button onClick={onFind} type="button">찾기</button>
      {bookmark}
      <div className="preview-more">
        <button
          aria-expanded={moreOpen}
          aria-haspopup="menu"
          onClick={() => setMoreOpen((open) => !open)}
          type="button"
        >
          더보기
        </button>
        {moreOpen && (
          <div className="preview-more-menu" role="menu">
            <button onClick={() => run(onOpenLocation)} role="menuitem" type="button">파일 위치 열기</button>
            <button onClick={() => run(onCopyText)} role="menuitem" type="button">텍스트 복사</button>
            <button onClick={() => run(onSaveMarkdown)} role="menuitem" type="button">Markdown 저장</button>
            <button onClick={() => run(onCopyPath)} role="menuitem" type="button">경로 복사</button>
            <button onClick={() => run(onAddTag)} role="menuitem" type="button">태그 추가</button>
          </div>
        )}
      </div>
    </div>
  );
}
