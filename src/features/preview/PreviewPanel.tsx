import { useEffect, useMemo, useState } from "react";
import { BookmarkButton } from "../library/BookmarkButton";
import { TagEditor } from "../library/TagEditor";
import {
  getPdfBytes,
  getPreview,
  openSourceFile,
  openSourceLocation,
  saveMarkdown,
} from "../../lib/ipc";
import type { PreviewBlock, PreviewDocument } from "../../lib/types";
import { DocumentTextView } from "./DocumentTextView";
import { PdfLayoutView, type PdfLoader } from "./PdfLayoutView";
import { PreviewToolbar } from "./PreviewToolbar";

function textFromBlocks(blocks: PreviewBlock[]): string {
  return blocks
    .flatMap((block) => {
      const tableText =
        block.table?.cells
          .map((row) => row.map((cell) => cell.text).join("\t"))
          .join("\n") ?? "";
      return [
        block.text,
        tableText,
        block.children ? textFromBlocks(block.children) : "",
      ];
    })
    .filter(Boolean)
    .join("\n\n");
}

interface PreviewPanelProps {
  documentId: string | null;
  getPreviewApi?: typeof getPreview;
  openApi?: typeof openSourceFile;
  openLocationApi?: typeof openSourceLocation;
  pdfBytesApi?: typeof getPdfBytes;
  saveMarkdownApi?: typeof saveMarkdown;
  pdfLoader?: PdfLoader;
}

export function PreviewPanel({
  documentId,
  getPreviewApi = getPreview,
  openApi = openSourceFile,
  openLocationApi = openSourceLocation,
  pdfBytesApi = getPdfBytes,
  saveMarkdownApi = saveMarkdown,
  pdfLoader,
}: PreviewPanelProps) {
  const [preview, setPreview] = useState<PreviewDocument | null>(null);
  const [tab, setTab] = useState<"text" | "layout">("text");
  const [findRequest, setFindRequest] = useState(0);
  const [tagOpen, setTagOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const plainText = useMemo(
    () => (preview ? textFromBlocks(preview.blocks) : ""),
    [preview],
  );

  useEffect(() => {
    if (!documentId) {
      setPreview(null);
      setError(null);
      return;
    }
    let active = true;
    setLoading(true);
    setError(null);
    setTab("text");
    void getPreviewApi(documentId)
      .then((result) => {
        if (active) setPreview(result);
      })
      .catch((caught) => {
        if (active) {
          setPreview(null);
          setError(caught instanceof Error ? caught.message : "미리보기를 불러오지 못했습니다.");
        }
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [documentId, getPreviewApi]);

  const run = async (action: () => Promise<unknown>) => {
    setError(null);
    try {
      await action();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "작업을 완료하지 못했습니다.");
    }
  };

  const copy = (value: string) =>
    run(async () => {
      if (!navigator.clipboard?.writeText) {
        throw new Error("클립보드를 사용할 수 없습니다.");
      }
      await navigator.clipboard.writeText(value);
    });

  if (!documentId) {
    return (
      <section className="preview-pane" aria-label="문서 미리보기 / Document preview">
        <div className="preview-empty">검색 결과에서 파일을 선택하면 내용을 볼 수 있습니다.</div>
      </section>
    );
  }
  if (loading) {
    return <section className="preview-pane" aria-label="문서 미리보기 / Document preview">미리보기 불러오는 중…</section>;
  }
  if (!preview) {
    return (
      <section className="preview-pane" aria-label="문서 미리보기 / Document preview">
        <div className="preview-message preview-message--error" role="alert">{error ?? "미리보기를 사용할 수 없습니다."}</div>
      </section>
    );
  }

  return (
    <section className="preview-pane" aria-label="문서 미리보기 / Document preview">
      <header className="preview-title">
        <strong title={preview.fileName}>{preview.fileName}</strong>
        <span>{preview.extension.toUpperCase()}</span>
      </header>
      <PreviewToolbar
        bookmark={
          <BookmarkButton
            bookmarked={preview.bookmarked}
            documentId={preview.documentId}
            onChange={(bookmarked) => setPreview((current) => current && ({ ...current, bookmarked }))}
          />
        }
        onAddTag={() => setTagOpen(true)}
        onCopyPath={() => void copy(preview.path)}
        onCopyText={() => void copy(plainText)}
        onFind={() => {
          setTab("text");
          setFindRequest((request) => request + 1);
        }}
        onOpen={() => void run(() => openApi(preview.documentId))}
        onOpenLocation={() => void run(() => openLocationApi(preview.documentId))}
        onSaveMarkdown={() => void run(() => saveMarkdownApi(preview.documentId))}
      />
      <TagEditor
        documentId={preview.documentId}
        onChange={(tags) => setPreview((current) => current && ({ ...current, tags }))}
        onOpenChange={setTagOpen}
        open={tagOpen}
        showTrigger={false}
        tags={preview.tags}
      />
      <div className="preview-tabs" role="tablist" aria-label="미리보기 형식">
        <button
          aria-selected={tab === "text"}
          onClick={() => setTab("text")}
          role="tab"
          type="button"
        >문서 텍스트</button>
        <button
          aria-selected={tab === "layout"}
          onClick={() => setTab("layout")}
          role="tab"
          type="button"
        >원본 레이아웃</button>
      </div>
      {error && <div className="preview-inline-error" role="alert">{error}</div>}
      <div className="preview-content" role="tabpanel">
        {tab === "text" ? (
          <DocumentTextView blocks={preview.blocks} findRequest={findRequest} />
        ) : preview.extension.toLocaleLowerCase() === "pdf" ? (
          <PdfLayoutView
            documentId={preview.documentId}
            getBytesApi={pdfBytesApi}
            loader={pdfLoader}
          />
        ) : (
          <div className="preview-layout-unavailable">
            <p>이 형식은 문서 텍스트로만 미리볼 수 있습니다.</p>
            <p>위의 파일 열기 버튼으로 원본 문서를 열 수 있습니다.</p>
          </div>
        )}
      </div>
    </section>
  );
}
