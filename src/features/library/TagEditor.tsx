import { useEffect, useState } from "react";
import { createTag, setDocumentTags } from "../../lib/ipc";
import type { Tag } from "../../lib/types";

interface TagEditorProps {
  documentId: string;
  tags: Tag[];
  onChange: (documentId: string, tags: Tag[]) => void;
  createApi?: typeof createTag;
  saveApi?: typeof setDocumentTags;
  open?: boolean;
  onOpenChange?: (documentId: string, open: boolean) => void;
  showTrigger?: boolean;
}

export function TagEditor({
  documentId,
  tags,
  onChange,
  createApi = createTag,
  saveApi = setDocumentTags,
  open,
  onOpenChange,
  showTrigger = true,
}: TagEditorProps) {
  const [internalOpen, setInternalOpen] = useState(false);
  const [name, setName] = useState("");
  const [color, setColor] = useState("terracotta");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const isOpen = open ?? internalOpen;
  const setOpen = (next: boolean) => {
    setInternalOpen(next);
    onOpenChange?.(documentId, next);
  };

  useEffect(() => {
    if (!isOpen) {
      setName("");
      setError(null);
    }
  }, [isOpen]);

  const createAndAttach = async () => {
    const trimmed = name.trim();
    if (!trimmed || busy) return;
    setBusy(true);
    setError(null);
    try {
      const created = await createApi(trimmed, color);
      const ids = [...new Set([...tags.map((tag) => tag.id), created.id])];
      const saved = await saveApi(documentId, ids);
      onChange(documentId, saved);
      setOpen(false);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "태그를 추가하지 못했습니다.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      {showTrigger && (
        <button onClick={() => setOpen(true)} type="button">태그 추가</button>
      )}
      {isOpen && (
        <div aria-label="태그 편집" className="tag-editor" role="dialog">
          <div className="tag-editor__heading">
            <strong>태그 추가</strong>
            <button aria-label="태그 편집 닫기" onClick={() => setOpen(false)} type="button">×</button>
          </div>
          <label>
            새 태그 이름
            <input
              aria-label="새 태그 이름"
              maxLength={64}
              onChange={(event) => setName(event.target.value)}
              value={name}
            />
          </label>
          <label>
            색상
            <select
              aria-label="태그 색상"
              onChange={(event) => setColor(event.target.value)}
              value={color}
            >
              <option value="terracotta">테라코타</option>
              <option value="amber">앰버</option>
              <option value="brown">브라운</option>
              <option value="sand">샌드</option>
              <option value="rose">로즈</option>
              <option value="slate">슬레이트</option>
              <option value="blue">블루</option>
              <option value="violet">바이올렛</option>
            </select>
          </label>
          {error && <p role="alert">{error}</p>}
          <button disabled={busy || !name.trim()} onClick={() => void createAndAttach()} type="button">
            태그 만들기
          </button>
        </div>
      )}
    </>
  );
}
