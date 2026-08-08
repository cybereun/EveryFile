import {
  createElement,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import type { PreviewBlock } from "../../lib/types";
import { useI18n } from "../../app/translations";

interface DocumentTextViewProps {
  blocks: PreviewBlock[];
  findRequest?: number;
  initialQuery?: string;
  preserveWhitespace?: boolean;
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

function renderableTableRows(table: NonNullable<PreviewBlock["table"]>) {
  const occupied = new Set<string>();
  return table.cells.map((row, rowIndex) =>
    row.flatMap((cell, columnIndex) => {
      const key = `${rowIndex}:${columnIndex}`;
      if (occupied.has(key)) return [];
      const colSpan = Math.max(1, cell.colSpan);
      const rowSpan = Math.max(1, cell.rowSpan);
      for (let rowOffset = 0; rowOffset < rowSpan; rowOffset += 1) {
        for (let columnOffset = 0; columnOffset < colSpan; columnOffset += 1) {
          if (rowOffset || columnOffset) {
            occupied.add(`${rowIndex + rowOffset}:${columnIndex + columnOffset}`);
          }
        }
      }
      return [{ cell, columnIndex, colSpan, rowSpan }];
    }),
  );
}

export function DocumentTextView({
  blocks,
  findRequest = 0,
  initialQuery = "",
  preserveWhitespace = false,
}: DocumentTextViewProps) {
  const { t } = useI18n();
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
    setQuery(initialQuery);
    setFindOpen(Boolean(initialQuery.trim()));
    setActiveMatch(0);
  }, [initialQuery, blocks]);

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
      case "table": {
        const table = block.table;
        if (!table) return null;
        return (
          <div className="document-table-scroll" key={key}>
            <table>
              <colgroup>
                {Array.from({ length: Math.max(1, table.cols) }, (_, index) => (
                  <col key={index} style={{ width: `${100 / Math.max(1, table.cols)}%` }} />
                ))}
              </colgroup>
              <tbody>
                {renderableTableRows(table).map((row, rowIndex) => (
                  <tr key={rowIndex}>
                    {row.map(({ cell, columnIndex, colSpan, rowSpan }) => {
                      const Tag = table.hasHeader && rowIndex === 0 ? "th" : "td";
                      return (
                        <Tag
                          colSpan={colSpan}
                          key={columnIndex}
                          rowSpan={rowSpan}
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
      }
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
        return <p key={key}>[{t("이미지")}] {linkedText(block)}</p>;
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
    <section
      className={`document-text-view${preserveWhitespace ? " document-text-view--plain" : ""}`}
      aria-label={t("문서 텍스트")}
    >
      {findOpen && (
        <div className="document-find">
          <input
            aria-label={t("문서 내 찾기")}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("문서 내 찾기")}
            ref={searchInput}
            type="search"
            value={query}
          />
          <span aria-live="polite">
            {totalMatches ? `${activeMatch + 1} / ${totalMatches}` : "0 / 0"}
          </span>
          <button aria-label={t("이전 일치")} onClick={() => moveMatch(-1)} type="button">↑</button>
          <button aria-label={t("다음 일치")} onClick={() => moveMatch(1)} type="button">↓</button>
          <button aria-label={t("찾기 닫기")} onClick={() => setFindOpen(false)} type="button">×</button>
        </div>
      )}
      <div className="document-blocks">
        {blocks.map((block, index) => renderBlock(block, String(index)))}
      </div>
    </section>
  );
}
