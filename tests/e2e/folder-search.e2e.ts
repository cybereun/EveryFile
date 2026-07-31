/// <reference types="mocha" />
import { expect } from "@wdio/globals";

describe("EveryFile folder indexing and content search", () => {
  it("indexes the fixture folder and finds a content-only phrase", async () => {
    const folder = await $(".folder-item");
    await folder.waitForDisplayed({ timeout: 30_000 });
    await expect(folder).toHaveText(expect.stringContaining("folder-search"));

    const searchInput = await $("input[aria-label='검색어']");
    await searchInput.waitForDisplayed();
    await searchInput.setValue("everyfile-local-content-phrase-7291");

    const result = await $(".search-result-row");
    await result.waitForDisplayed({ timeout: 45_000 });
    await expect(result).toHaveText(expect.stringContaining("fixture-note.md"));
  });
});
