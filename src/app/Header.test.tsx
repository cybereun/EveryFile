import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Header } from "./Header";

function renderCompactHeader(overrides: {
  onStatistics?: () => void;
  onAddFolder?: () => void;
  onSettings?: () => void;
} = {}) {
  return render(
    <Header
      compact
      locale="ko"
      tagline="파일을 찾는 가장 빠른 방법"
      onHome={() => undefined}
      onLocaleChange={() => undefined}
      {...overrides}
    />,
  );
}

describe("compact Header menu", () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("closes with Escape, restores trigger focus, and cleans up listeners", () => {
    renderCompactHeader();
    const more = screen.getByRole("button", { name: "더보기 / More" });
    more.focus();
    const addListener = vi.spyOn(document, "addEventListener");
    const removeListener = vi.spyOn(document, "removeEventListener");

    fireEvent.click(more);
    expect(
      screen.getByRole("group", { name: "추가 메뉴 / More actions" }),
    ).toBeVisible();
    expect(
      addListener.mock.calls.filter(([type]) =>
        ["keydown", "pointerdown"].includes(type),
      ),
    ).toHaveLength(2);

    fireEvent.keyDown(document, { key: "Escape" });

    expect(
      screen.queryByRole("group", { name: "추가 메뉴 / More actions" }),
    ).not.toBeInTheDocument();
    expect(more).toHaveFocus();
    expect(
      removeListener.mock.calls.filter(([type]) =>
        ["keydown", "pointerdown"].includes(type),
      ),
    ).toHaveLength(2);
  });

  it("closes on an outside pointer interaction", () => {
    renderCompactHeader();
    fireEvent.click(screen.getByRole("button", { name: "더보기 / More" }));
    expect(
      screen.getByRole("group", { name: "추가 메뉴 / More actions" }),
    ).toBeVisible();

    fireEvent.pointerDown(document.body);

    expect(
      screen.queryByRole("group", { name: "추가 메뉴 / More actions" }),
    ).not.toBeInTheDocument();
  });

  it("runs an enabled menu action and closes the menu", () => {
    const onStatistics = vi.fn();
    renderCompactHeader({ onStatistics });
    fireEvent.click(screen.getByRole("button", { name: "더보기 / More" }));

    fireEvent.click(screen.getByRole("button", { name: "통계 / Statistics" }));

    expect(onStatistics).toHaveBeenCalledOnce();
    expect(
      screen.queryByRole("group", { name: "추가 메뉴 / More actions" }),
    ).not.toBeInTheDocument();
  });
});
