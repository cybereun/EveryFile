import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ResizablePane } from "./ResizablePane";

describe("ResizablePane", () => {
  afterEach(cleanup);

  it("reverses keyboard direction for a left-edge handle", () => {
    const onWidthChange = vi.fn();
    render(
      <ResizablePane
        label="Preview resize"
        maxWidth={500}
        minWidth={280}
        onWidthChange={onWidthChange}
        resizeEdge="left"
        width={320}
      >
        Preview
      </ResizablePane>,
    );

    const separator = screen.getByRole("separator", { name: "Preview resize" });
    fireEvent.keyDown(separator, { key: "ArrowLeft" });
    expect(onWidthChange).toHaveBeenLastCalledWith(336);
    fireEvent.keyDown(separator, { key: "Home" });
    expect(onWidthChange).toHaveBeenLastCalledWith(280);
    fireEvent.keyDown(separator, { key: "End" });
    expect(onWidthChange).toHaveBeenLastCalledWith(500);
  });

  it("clamps pointer resizing and releases pointer capture", () => {
    const onWidthChange = vi.fn();
    render(
      <ResizablePane
        label="Folder resize"
        maxWidth={420}
        minWidth={208}
        onWidthChange={onWidthChange}
        resizeEdge="right"
        width={260}
      >
        Folders
      </ResizablePane>,
    );

    const separator = screen.getByRole("separator", { name: "Folder resize" });
    const setPointerCapture = vi.fn();
    const releasePointerCapture = vi.fn();
    Object.assign(separator, {
      setPointerCapture,
      hasPointerCapture: () => true,
      releasePointerCapture,
    });

    fireEvent.pointerDown(separator, { clientX: 100, pointerId: 7 });
    fireEvent.pointerMove(separator, { clientX: 400, pointerId: 7 });
    expect(onWidthChange).toHaveBeenLastCalledWith(420);
    fireEvent.pointerUp(separator, { clientX: 400, pointerId: 7 });
    expect(setPointerCapture).toHaveBeenCalledWith(7);
    expect(releasePointerCapture).toHaveBeenCalledWith(7);
  });
});
