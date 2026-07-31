import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DocumentStatistics } from "../../lib/types";
import { StatisticsDialog } from "./StatisticsDialog";

const statistics: DocumentStatistics = {
  totalDocuments: "3",
  indexedDocuments: "2",
  totalBytes: "42",
  byExtension: [
    { label: "pdf", count: "2" },
    { label: "(none)", count: "1" },
  ],
  byFolder: [{ id: "documents", label: "Documents", count: "3" }],
  byYear: [{ label: "2026", count: "3" }],
  recentlyModified: [],
  largestDocuments: [],
  parseStates: [{ label: "indexed", count: "2" }],
  totalSearches: "4",
  uniqueSearchTerms: "2",
  frequentSearches: [{ query: "report", count: "3", lastSearchedAt: "2026-01-01T00:00:00Z" }],
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
            resultCount: "2",
            elapsedMs: "5",
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

  it("submits an explicit extensionless filter for the extensionless bucket", async () => {
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

    fireEvent.click(
      await screen.findByRole("button", { name: "(NONE) 문서 1개 검색" }),
    );
    expect(apply).toHaveBeenCalledWith({ extensionless: true });
  });

  it("renders aggregate decimal strings without losing integers above 2^53", async () => {
    render(
      <StatisticsDialog
        open
        onClose={() => undefined}
        loadStatistics={async () => ({
          ...statistics,
          totalDocuments: "9007199254740993",
          totalBytes: "9007199254740993123",
        })}
        loadHistory={async () => []}
      />,
    );

    expect(await screen.findByText("9,007,199,254,740,993")).toBeVisible();
    expect(screen.getByText(/7\.81 EB/)).toBeVisible();
  });

  it("clears both recent and frequent history views after a successful clear", async () => {
    const clear = vi.fn(async () => undefined);
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
            resultCount: "2",
            elapsedMs: "5",
            searchedAt: "2026-01-01T00:00:00Z",
          },
        ]}
        clearHistory={clear}
      />,
    );
    fireEvent.click(await screen.findByRole("tab", { name: "검색 히스토리" }));
    expect(await screen.findByText("자주 검색")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "전체 삭제" }));

    expect(clear).toHaveBeenCalledOnce();
    expect(await screen.findByText("저장된 검색 히스토리가 없습니다.")).toBeVisible();
    expect(screen.queryByText("자주 검색")).not.toBeInTheDocument();
  });

  it("shows the private-search policy and excludes stale folder buckets", async () => {
    render(
      <StatisticsDialog
        open
        onClose={() => undefined}
        loadStatistics={async () => ({
          ...statistics,
          byFolder: [
            ...statistics.byFolder,
            { id: "removed", label: "Removed", count: "9" },
          ],
        })}
        loadHistory={async () => []}
        registeredFolderIds={["documents"]}
      />,
    );

    expect(await screen.findByText("Documents")).toBeVisible();
    expect(screen.queryByText("Removed")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("tab", { name: "검색 히스토리" }));
    expect(
      await screen.findByText(/비공개 검색은 기록에 저장되지 않으며/),
    ).toBeVisible();
  });

  it("supports arrow-key tab navigation", async () => {
    render(
      <StatisticsDialog
        open
        onClose={() => undefined}
        loadStatistics={async () => statistics}
        loadHistory={async () => []}
      />,
    );
    const documents = await screen.findByRole("tab", { name: "문서 통계" });
    fireEvent.keyDown(documents, { key: "ArrowRight" });
    expect(screen.getByRole("tab", { name: "검색 히스토리" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });
});
