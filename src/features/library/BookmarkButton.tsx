import { useState } from "react";
import { removeBookmark, setBookmark } from "../../lib/ipc";

interface BookmarkButtonProps {
  documentId: string;
  bookmarked: boolean;
  onChange: (bookmarked: boolean) => void;
  setApi?: typeof setBookmark;
  removeApi?: typeof removeBookmark;
}

export function BookmarkButton({
  documentId,
  bookmarked,
  onChange,
  setApi = setBookmark,
  removeApi = removeBookmark,
}: BookmarkButtonProps) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const toggle = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      if (bookmarked) {
        await removeApi(documentId);
        onChange(false);
      } else {
        await setApi(documentId, "");
        onChange(true);
      }
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "북마크를 변경하지 못했습니다.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <button
        aria-pressed={bookmarked}
        className="preview-action"
        disabled={busy}
        onClick={() => void toggle()}
        type="button"
      >
        {bookmarked ? "북마크 제거" : "북마크 추가"}
      </button>
      {error && <span className="preview-inline-error" role="alert">{error}</span>}
    </>
  );
}
