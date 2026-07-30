import { cleanup, render, screen } from "@testing-library/react";
// @ts-expect-error Vitest provides Node's built-in modules; the browser app omits Node types.
import { readFileSync } from "node:fs";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { App } from "./App";

const appCss = readFileSync("src/styles/app.css", "utf8");
const tokensCss = readFileSync("src/styles/tokens.css", "utf8");

describe("App accessibility", () => {
  beforeEach(() => {
    Object.defineProperty(window, "innerWidth", {
      configurable: true,
      value: 1440,
      writable: true,
    });
  });

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

  it("uses compliant dark text and control boundaries without changing tokens", () => {
    expect(tokensCss).toContain("--color-text: #46372d;");
    expect(tokensCss).toContain("--color-text-muted: #806f60;");
    expect(tokensCss).toContain("--color-border: #d9c4a4;");
    expect(appCss).toMatch(
      /\.folder-empty\s*\{[^}]*color:\s*var\(--color-text\)/s,
    );
    expect(appCss).toMatch(
      /\.search-field\s*\{[^}]*border:\s*1px solid var\(--color-text-muted\)/s,
    );
  });
});
