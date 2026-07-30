import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Tag } from "../../lib/types";
import { BookmarkButton } from "./BookmarkButton";
import { TagEditor } from "./TagEditor";

describe("library actions", () => {
  afterEach(cleanup);

  it("adds and removes a bookmark through document-scoped APIs", async () => {
    const add = vi.fn().mockResolvedValue(undefined);
    const remove = vi.fn().mockResolvedValue(undefined);
    const { rerender } = render(
      <BookmarkButton
        bookmarked={false}
        documentId="doc-1"
        onChange={vi.fn()}
        removeApi={remove}
        setApi={add}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "북마크 추가" }));
    await waitFor(() => expect(add).toHaveBeenCalledWith("doc-1", ""));

    rerender(
      <BookmarkButton
        bookmarked
        documentId="doc-1"
        onChange={vi.fn()}
        removeApi={remove}
        setApi={add}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "북마크 제거" }));
    await waitFor(() => expect(remove).toHaveBeenCalledWith("doc-1"));
  });

  it("creates a palette tag and saves selected tag IDs", async () => {
    const tag: Tag = { id: "tag-1", name: "검토", color: "terracotta" };
    const create = vi.fn().mockResolvedValue(tag);
    const save = vi.fn().mockResolvedValue([tag]);
    render(
      <TagEditor
        documentId="doc-1"
        onChange={vi.fn()}
        tags={[]}
        createApi={create}
        saveApi={save}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "태그 추가" }));
    fireEvent.change(screen.getByRole("textbox", { name: "새 태그 이름" }), {
      target: { value: "검토" },
    });
    fireEvent.click(screen.getByRole("button", { name: "태그 만들기" }));

    await waitFor(() => expect(create).toHaveBeenCalledWith("검토", "terracotta"));
    expect(save).toHaveBeenCalledWith("doc-1", ["tag-1"]);
  });
});
