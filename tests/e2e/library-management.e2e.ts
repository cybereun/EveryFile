/// <reference types="mocha" />
import { expect } from "@wdio/globals";

describe("EveryFile packaged library management", () => {
  it("persists bookmarks and tags, and reuses local search history/statistics", async () => {
    const query = "everyfile-local-content-phrase-7291";
    const search = await $("input[aria-label='검색어']");
    await search.setValue(query);
    const result = await $(".search-result-row");
    await result.waitForDisplayed({ timeout: 60_000 });
    await result.click();

    const bookmark = await $("button=북마크 추가");
    if (await bookmark.isExisting()) {
      await bookmark.click();
      await expect(await $("button=북마크 제거")).toBeDisplayed();
    }
    const toolbar = await $(".preview-toolbar");
    await toolbar.$(".preview-more > button").click();
    await toolbar.$("button=태그 추가").click();
    const tagName = await $("input[aria-label='새 태그 이름']");
    await tagName.setValue("E2E 확인");
    await $("button=태그 만들기").click();
    await expect(await $(".preview-tags")).toHaveText(
      expect.stringContaining("E2E 확인"),
    );

    await browser.refresh();
    const refreshedSearch = await $("input[aria-label='검색어']");
    await refreshedSearch.setValue(query);
    const refreshedResult = await $(".search-result-row");
    await refreshedResult.waitForDisplayed({ timeout: 60_000 });
    await refreshedResult.click();
    await expect(await $("button=북마크 제거")).toBeDisplayed();
    await expect(await $(".preview-tags")).toHaveText(
      expect.stringContaining("E2E 확인"),
    );

    await $("button[aria-label='통계 / Statistics']").click();
    await expect(await $("[aria-label='문서 통계 요약']")).toBeDisplayed();
    await $("button=검색 히스토리").click();
    await expect(await $("[aria-label='검색 히스토리']")).toHaveText(
      expect.stringContaining(query),
    );
  });
});
