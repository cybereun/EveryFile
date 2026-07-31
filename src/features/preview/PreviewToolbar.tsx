import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";

interface PreviewToolbarProps {
  bookmark: ReactNode;
  onAddTag: () => void;
  onCopyPath: () => void;
  onCopyText: () => void;
  onFind: () => void;
  onAiSummary?: () => void;
  onAiQuestion?: () => void;
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
  onAiSummary,
  onAiQuestion,
  onOpen,
  onOpenLocation,
  onSaveMarkdown,
}: PreviewToolbarProps) {
  const [moreOpen, setMoreOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const moreButton = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!moreOpen) return;
    const close = (event: MouseEvent) => {
      if (!root.current?.contains(event.target as Node)) setMoreOpen(false);
    };
    const keyboard = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setMoreOpen(false);
        moreButton.current?.focus();
        return;
      }
      const items = Array.from(
        menu.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]') ?? [],
      );
      if (items.length === 0) return;
      const current = items.indexOf(document.activeElement as HTMLButtonElement);
      let next = current;
      if (event.key === "ArrowDown") next = (current + 1) % items.length;
      else if (event.key === "ArrowUp") next = (current - 1 + items.length) % items.length;
      else if (event.key === "Home") next = 0;
      else if (event.key === "End") next = items.length - 1;
      else return;
      event.preventDefault();
      items[next]?.focus();
    };
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", keyboard);
    queueMicrotask(() =>
      menu.current?.querySelector<HTMLButtonElement>('[role="menuitem"]')?.focus(),
    );
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", keyboard);
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
      {onAiSummary && (
        <button onClick={onAiSummary} type="button">AI 요약</button>
      )}
      {onAiQuestion && (
        <button onClick={onAiQuestion} type="button">이 파일에 질문</button>
      )}
      {bookmark}
      <div className="preview-more">
        <button
          aria-expanded={moreOpen}
          aria-haspopup="menu"
          onClick={() => setMoreOpen((open) => !open)}
          ref={moreButton}
          type="button"
        >
          더보기
        </button>
        {moreOpen && (
          <div
            aria-label="추가 문서 작업"
            className="preview-more-menu"
            ref={menu}
            role="menu"
          >
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
