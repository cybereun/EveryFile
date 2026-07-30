import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DocumentStatistics } from "../../lib/types";
import { StatisticsDialog } from "./StatisticsDialog";

const statistics: DocumentStatistics = {
  totalDocuments: 3,
  indexedDocuments: 2,
  totalBytes: 42,
  byExtension: [{ label: "pdf", count: 2 }],
  byFolder: [{ id: "documents", label: "Documents", count: 3 }],
  byYear: [{ label: "2026", count: 3 }],
  recentlyModified: [],
  largestDocuments: [],
  parseStates: [{ label: "indexed", count: 2 }],
  totalSearches: 4,
  uniqueSearchTerms: 2,
  frequentSearches: [{ query: "report", count: 3, lastSearchedAt: "2026-01-01T00:00:00Z" }],
  recentSearches: [],
};

describe("StatisticsDialog", () => {
  afterEach(cleanup);

  it("renders chart data as an accessible table and applies a segment filter", async () => {
    const apply = vi.fn();
    render(
      <StatisticsDialog
        open
        onClose={() => undefined}
        loadStatistics={async () => statistics}
        loadHistory={async () => []}
        onApplyFilter={apply}
      />,
    );

    expect(await screen.findByRole("table", { name: "파일 유형별 문서 수" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "PDF 문서 2개 검색" }));
    expect(apply).toHaveBeenCalledWith({ extensions: ["pdf"] });
  });

  it("shows history and lets the user delete one entry", async () => {
    const remove = vi.fn(async () => undefined);
    render(
      <StatisticsDialog
        open
        onClose={() => undefined}
        loadStatistics={async () => statistics}
        loadHistory={async () => [
          {
            id: "history-1",
            query: "report",
            mode: "keyword",
            filters: {},
            resultCount: 2,
            elapsedMs: 5,
            searchedAt: "2026-01-01T00:00:00Z",
          },
        ]}
        deleteHistory={remove}
      />,
    );
    fireEvent.click(await screen.findByRole("tab", { name: "검색 히스토리" }));
    expect(await screen.findByText("report")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "report 기록 삭제" }));
    expect(remove).toHaveBeenCalledWith("history-1");
  });
});
