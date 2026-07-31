import { useEffect, useMemo, useRef, useState } from "react";
import { BookmarkButton } from "../library/BookmarkButton";
import { TagEditor } from "../library/TagEditor";
import {
  getPdfBytes,
  getPreview,
  openSourceFile,
  openSourceLocation,
  createTag,
  setBookmark,
  setDocumentTags,
  saveMarkdown,
  exportResults,
  runDocumentAi,
  cancelDocumentAi,
} from "../../lib/ipc";
import type { PreviewBlock, PreviewDocument } from "../../lib/types";
import { DocumentTextView } from "./DocumentTextView";
import { PdfLayoutView, type PdfLoader } from "./PdfLayoutView";
import { PreviewToolbar } from "./PreviewToolbar";
import { DocumentAiPanel } from "./DocumentAiPanel";

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
  createTagApi?: typeof createTag;
  setBookmarkApi?: typeof setBookmark;
  setTagsApi?: typeof setDocumentTags;
  aiEnabled?: boolean;
  aiProvider?: "ollama" | "gemini" | "openai";
  runAiApi?: typeof runDocumentAi;
  cancelAiApi?: typeof cancelDocumentAi;
}

export function PreviewPanel({
  documentId,
  getPreviewApi = getPreview,
  openApi = openSourceFile,
  openLocationApi = openSourceLocation,
  pdfBytesApi = getPdfBytes,
  saveMarkdownApi = saveMarkdown,
  pdfLoader,
  createTagApi = createTag,
  setBookmarkApi = setBookmark,
  setTagsApi = setDocumentTags,
  aiEnabled = false,
  aiProvider = "ollama",
  runAiApi = runDocumentAi,
  cancelAiApi = cancelDocumentAi,
}: PreviewPanelProps) {
  const [preview, setPreview] = useState<PreviewDocument | null>(null);
  const [tab, setTab] = useState<"text" | "layout">("text");
  const [findRequest, setFindRequest] = useState(0);
  const [tagOpen, setTagOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [aiMode, setAiMode] = useState<"summary" | "question" | null>(null);
  const selectedDocumentId = useRef(documentId);
  selectedDocumentId.current = documentId;
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
    setAiMode(null);
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

  const run = async (ownerDocumentId: string, action: () => Promise<unknown>) => {
    if (selectedDocumentId.current === ownerDocumentId) setError(null);
    try {
      await action();
    } catch (caught) {
      if (selectedDocumentId.current === ownerDocumentId) {
        setError(caught instanceof Error ? caught.message : "작업을 완료하지 못했습니다.");
      }
    }
  };

  const copy = (ownerDocumentId: string, value: string) =>
    run(ownerDocumentId, async () => {
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
            key={preview.documentId}
            bookmarked={preview.bookmarked}
            documentId={preview.documentId}
            setApi={setBookmarkApi}
            onChange={(ownerDocumentId, bookmarked) => {
              if (selectedDocumentId.current !== ownerDocumentId) return;
              setPreview((current) =>
                current?.documentId === ownerDocumentId
                  ? { ...current, bookmarked }
                  : current,
              );
            }}
          />
        }
        onAddTag={() => setTagOpen(true)}
        onCopyPath={() => void copy(preview.documentId, preview.path)}
        onCopyText={() => void copy(preview.documentId, plainText)}
        onFind={() => {
          setTab("text");
          setFindRequest((request) => request + 1);
        }}
        onAiSummary={
          aiEnabled
            ? () => {
                setAiMode("summary");
              }
            : undefined
        }
        onAiQuestion={
          aiEnabled
            ? () => {
                setAiMode("question");
              }
            : undefined
        }
        onOpen={() => void run(preview.documentId, () => openApi(preview.documentId))}
        onOpenLocation={() => void run(preview.documentId, () => openLocationApi(preview.documentId))}
        onSaveMarkdown={() =>
          void run(preview.documentId, () =>
            saveMarkdownApi === saveMarkdown
              ? exportResults(
                  {
                    kind: "markdownDocument",
                    fileName: preview.fileName,
                    markdown: preview.markdown,
                  },
                  "markdown",
                )
              : saveMarkdownApi(preview.documentId),
          )
        }
      />
      {aiEnabled && aiMode && (
        <DocumentAiPanel
          cancelApi={cancelAiApi}
          documentId={preview.documentId}
          mode={aiMode}
          onClose={() => setAiMode(null)}
          provider={aiProvider}
          runApi={runAiApi}
        />
      )}
      <TagEditor
        key={preview.documentId}
        createApi={createTagApi}
        documentId={preview.documentId}
        onChange={(ownerDocumentId, tags) => {
          if (selectedDocumentId.current !== ownerDocumentId) return;
          setPreview((current) =>
            current?.documentId === ownerDocumentId ? { ...current, tags } : current,
          );
        }}
        onOpenChange={(ownerDocumentId, open) => {
          if (selectedDocumentId.current === ownerDocumentId) setTagOpen(open);
        }}
        open={tagOpen}
        saveApi={setTagsApi}
        showTrigger={false}
        tags={preview.tags}
      />
      {preview.tags.length > 0 && (
        <div className="preview-tags" aria-label="문서 태그">
          {preview.tags.map((tag) => (
            <span className="preview-tag" key={tag.id}>{tag.name}</span>
          ))}
        </div>
      )}
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
      {preview.truncated && (
        <div className="preview-limit-notice" role="status">
          문서가 커서 안전한 미리보기 한도까지만 표시합니다.
        </div>
      )}
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
            <button
              onClick={() =>
                void run(preview.documentId, () => openApi(preview.documentId))
              }
              type="button"
            >
              파일 열기
            </button>
          </div>
        )}
      </div>
    </section>
  );
}
