import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { App } from "./App";

describe("App", () => {
  it("renders the EveryFile product identity", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "EveryFile" })).toBeVisible();
    expect(screen.getByText("?뚯씪??李얜뒗 媛??鍮좊Ⅸ 諛⑸쾿")).toBeVisible();
  });
});
