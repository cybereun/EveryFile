import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "./App";

describe("App accessibility", () => {
  afterEach(cleanup);

  it("gives every icon-only header control a bilingual accessible name", () => {
    render(<App />);

    for (const name of [
      "홈 / Home",
      "통계 / Statistics",
      "폴더 추가 / Add folder",
      "설정 / Settings",
    ]) {
      expect(screen.getByRole("button", { name })).toBeVisible();
    }
  });

  it("provides labelled landmarks and keyboard-operable resize controls", () => {
    render(<App />);

    expect(screen.getByRole("banner")).toBeVisible();
    expect(screen.getByRole("main")).toBeVisible();
    expect(screen.getByRole("contentinfo")).toBeVisible();
    expect(
      screen.getByRole("separator", {
        name: "폴더 패널 크기 조절 / Resize folder pane",
      }),
    ).toHaveAttribute("tabindex", "0");
    expect(
      screen.getByRole("separator", {
        name: "미리보기 패널 크기 조절 / Resize preview pane",
      }),
    ).toHaveAttribute("tabindex", "0");
  });
});
