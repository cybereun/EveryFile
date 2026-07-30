import {
  createElement,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import type { PreviewBlock } from "../../lib/types";

interface DocumentTextViewProps {
  blocks: PreviewBlock[];
  findRequest?: number;
}

function safeHref(href: string | null | undefined) {
  if (!href || /[\u0000-\u001f\u007f]/.test(href)) return null;
  try {
    const parsed = new URL(href);
    return ["http:", "https:", "mailto:"].includes(parsed.protocol) ? href : null;
  } catch {
    return null;
  }
}

function blockTexts(block: PreviewBlock): string[] {
  const cells =
    block.table?.cells.flatMap((row) => row.map((cell) => cell.text)) ?? [];
  return [
    block.text,
    ...cells,
    ...(block.children?.flatMap(blockTexts) ?? []),
  ].filter(Boolean);
}

function countOccurrences(text: string, query: string) {
  if (!query) return 0;
  const source = text.toLocaleLowerCase();
  const target = query.toLocaleLowerCase();
  let count = 0;
  let offset = 0;
  while ((offset = source.indexOf(target, offset)) !== -1) {
    count += 1;
    offset += Math.max(1, target.length);
  }
  return count;
}

export function DocumentTextView({
  blocks,
  findRequest = 0,
}: DocumentTextViewProps) {
  const [findOpen, setFindOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [activeMatch, setActiveMatch] = useState(0);
  const searchInput = useRef<HTMLInputElement>(null);
  const activeMark = useRef<HTMLElement | null>(null);
  const totalMatches = useMemo(
    () =>
      query
        ? blocks
            .flatMap(blockTexts)
            .reduce((total, text) => total + countOccurrences(text, query), 0)
        : 0,
    [blocks, query],
  );

  useEffect(() => {
    const handleFind = (event: KeyboardEvent) => {
      if (
        event.ctrlKey &&
        !event.altKey &&
        !event.metaKey &&
        event.key.toLocaleLowerCase() === "f"
      ) {
        event.preventDefault();
        setFindOpen(true);
        window.setTimeout(() => searchInput.current?.focus(), 0);
      } else if (event.key === "Escape" && findOpen) {
        setFindOpen(false);
      }
    };
    window.addEventListener("keydown", handleFind);
    return () => window.removeEventListener("keydown", handleFind);
  }, [findOpen]);

  useEffect(() => {
    if (findRequest > 0) {
      setFindOpen(true);
      window.setTimeout(() => searchInput.current?.focus(), 0);
    }
  }, [findRequest]);

  useEffect(() => {
    setActiveMatch(0);
  }, [query]);

  useEffect(() => {
    if (typeof activeMark.current?.scrollIntoView === "function") {
      activeMark.current.scrollIntoView({ block: "center" });
    }
  }, [activeMatch, query]);

  let matchIndex = 0;
  const renderText = (text: string): ReactNode => {
    if (!query) return text;
    const source = text.toLocaleLowerCase();
    const target = query.toLocaleLowerCase();
    const output: ReactNode[] = [];
    let cursor = 0;
    let index = source.indexOf(target);
    while (index !== -1) {
      if (index > cursor) output.push(text.slice(cursor, index));
      const current = matchIndex++;
      output.push(
        <mark
          className={current === activeMatch ? "is-active" : undefined}
          key={`${index}-${current}`}
          ref={(element) => {
            if (current === activeMatch) activeMark.current = element;
          }}
        >
          {text.slice(index, index + query.length)}
        </mark>,
      );
      cursor = index + query.length;
      index = source.indexOf(target, cursor);
    }
    output.push(text.slice(cursor));
    return output;
  };

  const linkedText = (block: PreviewBlock) => {
    const href = safeHref(block.href);
    const text = renderText(block.text);
    return href ? (
      <a href={href} rel="noreferrer noopener" target="_blank">{text}</a>
    ) : text;
  };

  const renderBlock = (block: PreviewBlock, key: string): ReactNode => {
    switch (block.type) {
      case "heading": {
        const level = Math.min(6, Math.max(1, block.level ?? 2));
        return createElement(`h${level}`, { key }, linkedText(block));
      }
      case "table":
        return (
          <div className="document-table-scroll" key={key}>
            <table>
              <tbody>
                {block.table?.cells.map((row, rowIndex) => (
                  <tr key={rowIndex}>
                    {row.map((cell, cellIndex) => {
                      const Tag = block.table?.hasHeader && rowIndex === 0 ? "th" : "td";
                      return (
                        <Tag
                          colSpan={Math.max(1, cell.colSpan)}
                          key={cellIndex}
                          rowSpan={Math.max(1, cell.rowSpan)}
                          scope={Tag === "th" ? "col" : undefined}
                        >
                          {renderText(cell.text)}
                        </Tag>
                      );
                    })}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        );
      case "list": {
        const List = block.listType === "ordered" ? "ol" : "ul";
        return (
          <List key={key}>
            <li>
              {linkedText(block)}
              {block.children?.map((child, index) =>
                renderBlock(child, `${key}-${index}`),
              )}
            </li>
          </List>
        );
      }
      case "separator":
        return <hr key={key} />;
      case "image":
        return <p key={key}>[이미지] {linkedText(block)}</p>;
      case "paragraph":
      default:
        return <p key={key}>{linkedText(block)}</p>;
    }
  };

  const moveMatch = (direction: number) => {
    if (!totalMatches) return;
    setActiveMatch((current) => (current + direction + totalMatches) % totalMatches);
  };

  return (
    <section className="document-text-view" aria-label="문서 텍스트">
      {findOpen && (
        <div className="document-find">
          <input
            aria-label="문서 내 찾기"
            onChange={(event) => setQuery(event.target.value)}
            placeholder="문서 내 찾기"
            ref={searchInput}
            type="search"
            value={query}
          />
          <span aria-live="polite">
            {totalMatches ? `${activeMatch + 1} / ${totalMatches}` : "0 / 0"}
          </span>
          <button aria-label="이전 일치" onClick={() => moveMatch(-1)} type="button">↑</button>
          <button aria-label="다음 일치" onClick={() => moveMatch(1)} type="button">↓</button>
          <button aria-label="찾기 닫기" onClick={() => setFindOpen(false)} type="button">×</button>
        </div>
      )}
      <div className="document-blocks">
        {blocks.map((block, index) => renderBlock(block, String(index)))}
      </div>
    </section>
  );
}
