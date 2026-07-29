import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "./App";

describe("App", () => {
  afterEach(cleanup);

  it("renders the Korean EveryFile product identity by default", () => {
    render(<App />);

    expect(screen.getByRole("heading", { name: "EveryFile" })).toBeVisible();
    expect(
      screen.getByText("\uD30C\uC77C\uC744 \uCC3E\uB294 \uAC00\uC7A5 \uBE60\uB978 \uBC29\uBC95"),
    ).toBeVisible();
  });

  it("renders the English identity after language selection", () => {
    render(<App />);

    fireEvent.change(screen.getByRole("combobox", { name: "Language" }), {
      target: { value: "en" },
    });

    expect(screen.getByText("The fastest way to find files.")).toBeVisible();
  });
});
