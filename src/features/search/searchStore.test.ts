import { describe, expect, it } from "vitest";
import {
  extensionsFromQuery,
  parseSearchQuery,
  queryForTermMode,
  removeQueryClause,
  serializeSearchQuery,
  withExtensionQuery,
} from "./searchStore";

describe("search query AST", () => {
  it("round-trips operators, quoted phrases, escapes, OR, near, and Windows paths", () => {
    const query =
      String.raw`"alpha \"quoted\"" OR beta gamma -draft ext:hwp,pdf path:"C:\My Files\교육" after:2026-01-01 before:2026-12-31 ~7`;
    const parsed = parseSearchQuery(query);
    const reparsed = parseSearchQuery(serializeSearchQuery(parsed));

    expect(reparsed.clauses).toEqual(parsed.clauses);
    expect(parsed.errors).toEqual([]);
  });

  it("does not interpret quoted operator-looking text as an operator", () => {
    const parsed = parseSearchQuery(
      String.raw`"ext:exe" "path:C:\escape" ext:pdf`,
    );

    expect(parsed.clauses.map((clause) => clause.kind)).toEqual([
      "phrase",
      "phrase",
      "extension",
    ]);
  });

  it("updates and removes only the targeted structured clause", () => {
    const query =
      String.raw`alpha path:"C:\My Files" after:2026-01-01 ext:hwp,pdf`;
    expect(withExtensionQuery(query, ["pdf"])).toBe(
      String.raw`alpha path:"C:\My Files" after:2026-01-01 ext:pdf`,
    );
    expect(
      removeQueryClause(query, (clause) => clause.kind === "path"),
    ).toBe("alpha after:2026-01-01 ext:hwp,pdf");
  });

  it("preserves explicit OR grouping instead of flattening mixed clauses", () => {
    const parsed = parseSearchQuery("alpha OR beta gamma");
    expect(parsed.positiveGroups).toEqual([["alpha"], ["beta", "gamma"]]);
  });

  it("normalizes conflicting syntax when the selected term mode takes precedence", () => {
    const query =
      String.raw`alpha OR "beta phrase" -draft ~7 path:"C:\My Files" after:2026-01-01 ext:pdf`;

    expect(queryForTermMode(query, "exact")).toBe(
      String.raw`alpha "beta phrase" draft path:"C:\My Files" after:2026-01-01 ext:pdf`,
    );
    expect(queryForTermMode(query, "near")).toBe(
      String.raw`alpha "beta phrase" draft path:"C:\My Files" after:2026-01-01 ext:pdf`,
    );
    expect(queryForTermMode(query, "exclude")).toBe(
      String.raw`alpha "beta phrase" -draft path:"C:\My Files" after:2026-01-01 ext:pdf`,
    );
    expect(queryForTermMode(query, "all")).toBe(
      String.raw`alpha "beta phrase" draft path:"C:\My Files" after:2026-01-01 ext:pdf`,
    );
  });

  it("decodes quoted extension values the same way as the Rust parser", () => {
    expect(extensionsFromQuery(String.raw`alpha ext:"pdf"`)).toEqual(["pdf"]);
    expect(extensionsFromQuery(String.raw`ext:"hwp,pdf"`)).toEqual([
      "hwp",
      "pdf",
    ]);
  });
});
