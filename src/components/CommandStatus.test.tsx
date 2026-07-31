import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CommandStatus } from "./CommandStatus";

describe("CommandStatus", () => {
  afterEach(cleanup);

  it("shows an actionable error and can be dismissed", () => {
    const onDismiss = vi.fn();
    render(
      <CommandStatus
        message={{ kind: "error", text: "폴더 등록에 실패했습니다." }}
        onDismiss={onDismiss}
      />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent(
      "폴더 등록에 실패했습니다.",
    );
    fireEvent.click(screen.getByRole("button", { name: /Dismiss/ }));
    expect(onDismiss).toHaveBeenCalledOnce();
  });
});
