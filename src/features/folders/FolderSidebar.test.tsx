import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { FolderRecord } from "../../lib/types";
import { FolderSidebar } from "./FolderSidebar";

const folders: FolderRecord[] = [
  {
    id: "folder-1",
    canonicalPath: "C:\\Users\\me\\Documents",
    displayName: "Documents",
    documentCount: 1234,
    indexState: "idle",
  },
  {
    id: "folder-2",
    canonicalPath: "D:\\Team",
    displayName: "Team",
    documentCount: 5,
    indexState: "idle",
  },
];

describe("FolderSidebar", () => {
  afterEach(cleanup);

  it("renders registered folders with localized counts", () => {
    render(<FolderSidebar folders={folders} onAdd={() => undefined} />);

    expect(
      screen.getByRole("complementary", { name: "\uB4F1\uB85D \uD3F4\uB354" }),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: /Documents/ })).toHaveTextContent(
      `Documents${(1234).toLocaleString()}`,
    );
    expect(screen.getByRole("button", { name: /Team/ })).toHaveTextContent(
      "Team5",
    );
  });

  it("calls the add action from the heading button", () => {
    const onAdd = vi.fn();
    render(<FolderSidebar folders={folders} onAdd={onAdd} />);

    fireEvent.click(
      screen.getByRole("button", { name: "\uD3F4\uB354 \uCD94\uAC00" }),
    );

    expect(onAdd).toHaveBeenCalledOnce();
  });

  it("offers one folder selection action and explains the empty scope", () => {
    const onAdd = vi.fn();
    render(<FolderSidebar folders={[]} onAdd={onAdd} />);

    expect(
      screen.getByText(
        "\uC120\uD0DD\uD55C \uD3F4\uB354\uB9CC \uC0C9\uC778\uB429\uB2C8\uB2E4.",
      ),
    ).toBeVisible();
    const actions = screen.getAllByRole("button", {
      name: "\uD3F4\uB354 \uC120\uD0DD",
    });
    expect(actions).toHaveLength(1);

    fireEvent.click(actions[0]);
    expect(onAdd).toHaveBeenCalledOnce();
  });
});
