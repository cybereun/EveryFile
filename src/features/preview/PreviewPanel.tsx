import { useEffect, useMemo, useRef, useState } from "react";
import { BookmarkButton } from "../library/BookmarkButton";
import { TagEditor } from "../library/TagEditor";
import {
  getPdfBytes,
  getImageBytes,
  getLayoutBytes,
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
  cancelPdfRead,
} from "../../lib/ipc";
import type { PreviewBlock, PreviewDocument } from "../../lib/types";
import { DocumentTextView } from "./DocumentTextView";
import { PdfLayoutView, type PdfLoader } from "./PdfLayoutView";
import { HwpLayoutView, type HwpLoader } from "./HwpLayoutView";
import { PreviewToolbar } from "./PreviewToolbar";
import { DocumentAiPanel } from "./DocumentAiPanel";
import { useI18n } from "../../app/translations";

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

function imageMimeType(extension: string) {
  switch (extension.toLocaleLowerCase()) {
    case "jpg":
    case "jpeg":
      return "image/jpeg";
    case "png":
      return "image/png";
    case "webp":
      return "image/webp";
    case "bmp":
      return "image/bmp";
    case "gif":
      return "image/gif";
    case "tif":
    case "tiff":
      return "image/tiff";
    case "svg":
      return "image/svg+xml";
    default:
      return "application/octet-stream";
  }
}

function isImageExtension(extension: string) {
  return ["jpg", "jpeg", "png", "webp", "bmp", "gif", "tif", "tiff", "svg"].includes(
    extension.toLocaleLowerCase(),
  );
}

let imageRequestSequence = 0;

function nextImageRequestId() {
  imageRequestSequence += 1;
  return `image-preview-${Date.now().toString(36)}-${imageRequestSequence.toString(36)}`;
}

function ImagePreview({
  documentId,
  fileName,
  extension,
  getBytesApi,
}: {
  documentId: string;
  fileName: string;
  extension: string;
  getBytesApi: typeof getImageBytes;
}) {
  const { t } = useI18n();
  const [source, setSource] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const sourceRef = useRef<string | null>(null);

  useEffect(() => {
    const requestId = nextImageRequestId();
    let active = true;
    setSource(null);
    setError(null);
    void getBytesApi(documentId, requestId)
      .then((bytes) => {
        const url = URL.createObjectURL(new Blob([bytes], { type: imageMimeType(extension) }));
        if (!active) {
          URL.revokeObjectURL(url);
          return;
        }
        if (sourceRef.current) URL.revokeObjectURL(sourceRef.current);
        sourceRef.current = url;
        setSource(url);
      })
      .catch((caught) => {
        if (active) setError(caught instanceof Error ? caught.message : t("이미지를 불러오지 못했습니다."));
      });
    return () => {
      active = false;
      void cancelPdfRead(requestId);
      if (sourceRef.current) {
        URL.revokeObjectURL(sourceRef.current);
        sourceRef.current = null;
      }
    };
  }, [documentId, extension, getBytesApi, t]);

  if (error) return <div className="preview-message preview-message--error" role="alert">{error}</div>;
  if (!source) return <div className="preview-message">{t("이미지 미리보기를 불러오는 중…")}</div>;
  return (
    <div className="image-preview" aria-label={`${fileName} ${t("이미지 미리보기")}`}>
      <img src={source} alt={fileName} decoding="async" />
    </div>
  );
}

function PreviewTabs({
  tab,
  onTabChange,
  disabled = false,
}: {
  tab: "text" | "layout";
  onTabChange: (next: "text" | "layout") => void;
  disabled?: boolean;
}) {
  const { t } = useI18n();
  return (
    <div className="preview-tabs" role="tablist" aria-label={t("미리보기 형식")}>
      <button
        aria-selected={tab === "text"}
        disabled={disabled}
        onClick={() => onTabChange("text")}
        role="tab"
        type="button"
      >{t("문서 텍스트")}</button>
      <button
        aria-selected={tab === "layout"}
        disabled={disabled}
        onClick={() => onTabChange("layout")}
        role="tab"
        type="button"
      >{t("원본 레이아웃")}</button>
    </div>
  );
}

interface PreviewPanelProps {
  documentId: string | null;
  getPreviewApi?: typeof getPreview;
  openApi?: typeof openSourceFile;
  openLocationApi?: typeof openSourceLocation;
  pdfBytesApi?: typeof getPdfBytes;
  imageBytesApi?: typeof getImageBytes;
  saveMarkdownApi?: typeof saveMarkdown;
  pdfLoader?: PdfLoader;
  hwpBytesApi?: typeof getLayoutBytes;
  hwpLoader?: HwpLoader;
  createTagApi?: typeof createTag;
  setBookmarkApi?: typeof setBookmark;
  setTagsApi?: typeof setDocumentTags;
  aiEnabled?: boolean;
  aiProvider?: "ollama" | "gemini" | "openai";
  runAiApi?: typeof runDocumentAi;
  cancelAiApi?: typeof cancelDocumentAi;
  searchQuery?: string;
  onBookmarkChanged?: () => void;
}

export function PreviewPanel({
  documentId,
  getPreviewApi = getPreview,
  openApi = openSourceFile,
  openLocationApi = openSourceLocation,
  pdfBytesApi = getPdfBytes,
  imageBytesApi = getImageBytes,
  saveMarkdownApi = saveMarkdown,
  pdfLoader,
  hwpBytesApi = getLayoutBytes,
  hwpLoader,
  createTagApi = createTag,
  setBookmarkApi = setBookmark,
  setTagsApi = setDocumentTags,
  aiEnabled = false,
  aiProvider = "ollama",
  runAiApi = runDocumentAi,
  cancelAiApi = cancelDocumentAi,
  searchQuery = "",
  onBookmarkChanged,
}: PreviewPanelProps) {
  const { t } = useI18n();
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
          setError(caught instanceof Error ? caught.message : t("미리보기를 불러오지 못했습니다."));
        }
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [documentId, getPreviewApi, t]);

  const run = async (ownerDocumentId: string, action: () => Promise<unknown>) => {
    if (selectedDocumentId.current === ownerDocumentId) setError(null);
    try {
      await action();
    } catch (caught) {
      if (selectedDocumentId.current === ownerDocumentId) {
        setError(caught instanceof Error ? caught.message : t("작업을 완료하지 못했습니다."));
      }
    }
  };

  const copy = (ownerDocumentId: string, value: string) =>
    run(ownerDocumentId, async () => {
      if (!navigator.clipboard?.writeText) {
        throw new Error(t("클립보드를 사용할 수 없습니다."));
      }
      await navigator.clipboard.writeText(value);
    });

  if (!documentId) {
    return (
      <section className="preview-pane" aria-label={t("문서 미리보기 / Document preview")}>
        <PreviewTabs tab={tab} onTabChange={setTab} disabled />
        <div className="preview-empty">{t("검색 결과에서 파일을 선택하면 내용을 볼 수 있습니다.")}</div>
      </section>
    );
  }
  if (!preview) {
    return (
      <section className="preview-pane" aria-label={t("문서 미리보기 / Document preview")}>
        <PreviewTabs tab={tab} onTabChange={setTab} disabled={loading} />
        <div
          className={loading ? "preview-message" : "preview-message preview-message--error"}
          role={loading ? "status" : "alert"}
        >
          {loading ? t("미리보기 불러오는 중…") : error ?? t("미리보기를 사용할 수 없습니다.")}
        </div>
      </section>
    );
  }

  return (
      <section className="preview-pane" aria-label={t("문서 미리보기 / Document preview")}>
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
              onBookmarkChanged?.();
            }}
          />
        }
        onAddTag={() => setTagOpen(true)}
        onCopyPath={() => void copy(preview.documentId, preview.path)}
        onCopyText={() => void copy(preview.documentId, plainText)}
        onFind={() => {
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
        <div className="preview-tags" aria-label={t("문서 태그")}>
          {preview.tags.map((tag) => (
            <span className="preview-tag" key={tag.id}>{tag.name}</span>
          ))}
        </div>
      )}
      <PreviewTabs tab={tab} onTabChange={setTab} disabled={loading} />
      {preview.truncated && (
        <div className="preview-limit-notice" role="status">
          {t("문서가 커서 안전한 미리보기 한도까지만 표시합니다.")}
        </div>
      )}
      {error && <div className="preview-inline-error" role="alert">{error}</div>}
      <div className="preview-content" aria-busy={loading} role="tabpanel">
        {loading ? (
          <div className="preview-message" role="status">
            {t("문서 미리보기를 불러오는 중…")}
          </div>
        ) : isImageExtension(preview.extension) ? (
          <ImagePreview
            documentId={preview.documentId}
            extension={preview.extension}
            fileName={preview.fileName}
            getBytesApi={imageBytesApi}
          />
        ) : tab === "text" ? (
          <DocumentTextView
            blocks={preview.blocks}
            findRequest={findRequest}
            initialQuery={searchQuery}
            preserveWhitespace={["txt", "md", "markdown"].includes(
              preview.extension.toLocaleLowerCase(),
            )}
          />
        ) : preview.extension.toLocaleLowerCase() === "pdf" ? (
          <PdfLayoutView
            documentId={preview.documentId}
            findRequest={findRequest}
            getBytesApi={pdfBytesApi}
            initialQuery={searchQuery}
            loader={pdfLoader}
          />
        ) : ["hwp", "hwpx"].includes(preview.extension.toLocaleLowerCase()) ? (
          <HwpLayoutView
            documentId={preview.documentId}
            fallbackText={plainText}
            findRequest={findRequest}
            getBytesApi={hwpBytesApi}
            initialQuery={searchQuery}
            loader={hwpLoader}
          />
        ) : (
          <div className="preview-layout-unavailable">
            <p>{t("이 형식은 문서 텍스트로만 미리볼 수 있습니다.")}</p>
            <button
              onClick={() =>
                void run(preview.documentId, () => openApi(preview.documentId))
              }
              type="button"
            >
              {t("파일 열기")}
            </button>
          </div>
        )}
      </div>
    </section>
  );
}
