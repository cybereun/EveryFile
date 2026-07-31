/// <reference types="mocha" />
import { expect } from "@wdio/globals";

describe("EveryFile packaged local OCR", () => {
  it("indexes scanned PDF, JPG, PNG, WebP, BMP, and TIFF text locally", async () => {
    const status = await $(".status-summary");
    await browser.waitUntil(
      async () => (await status.getText()).includes("색인 문서 7개"),
      {
        timeout: 120_000,
        interval: 1_000,
        timeoutMsg: "OCR fixture indexing did not finish",
      },
    );
    const searchInput = await $("input[aria-label='검색어']");
    await searchInput.waitForDisplayed();
    await searchInput.setValue("everyfile ocr acceptance 8427");

    await browser.waitUntil(
      async () => (await $$(".search-result-row")).length >= 6,
      {
        timeout: 30_000,
        interval: 500,
        timeoutMsg: "all six OCR fixture formats were not searchable",
      },
    );
    const results = await $$(".search-result-row");
    expect(results.length).toBeGreaterThanOrEqual(6);
  });
});
