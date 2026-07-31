/// <reference types="mocha" />
import { expect } from "@wdio/globals";

describe("EveryFile independent workspace panels", () => {
  it("toggles the panels, persists choices, and exposes preview actions", async () => {
    await browser.setWindowSize(1440, 900);
    const leftButton = await $("button[aria-label*='왼쪽 패널']");
    const rightButton = await $("button[aria-label*='오른쪽 패널']");
    const leftPane = await $(".left-pane-container");
    const rightPane = await $(".preview-pane-container");

    if ((await leftButton.getAttribute("aria-pressed")) !== "true") {
      await leftButton.click();
    }
    if ((await rightButton.getAttribute("aria-pressed")) !== "true") {
      await rightButton.click();
    }
    await expect(leftPane).toBeDisplayed();

    await leftButton.click();
    await expect(leftPane).not.toBeDisplayed();
    await expect(rightPane).toBeDisplayed();

    await rightButton.click();
    await expect(rightPane).not.toBeDisplayed();

    await browser.refresh();
    await expect(leftPane).not.toBeDisplayed();
    await expect(rightPane).not.toBeDisplayed();

    await rightButton.click();
    const searchInput = await $("input[aria-label='검색어']");
    await searchInput.setValue("everyfile-local-content-phrase-7291");
    const result = await $(".search-result-row");
    await result.waitForDisplayed({ timeout: 45_000 });
    await result.click();

    await expect($(".preview-title")).toHaveText(expect.stringContaining("fixture-note.md"));
    const toolbar = await $(".preview-toolbar");
    await expect(toolbar).toHaveText(expect.stringContaining("파일 열기"));
    await expect(toolbar).toHaveText(expect.stringContaining("찾기"));
    const toolbarText = await toolbar.getText();
    expect(
      toolbarText.includes("북마크 추가") || toolbarText.includes("북마크 제거"),
    ).toBe(true);
    await toolbar.$(".preview-more > button").click();
    const menu = await toolbar.$("[role='menu']");
    await expect(menu).toHaveText(expect.stringContaining("파일 위치 열기"));
    await expect(menu).toHaveText(expect.stringContaining("텍스트 복사"));
    await expect(menu).toHaveText(expect.stringContaining("Markdown 저장"));
    await expect(menu).toHaveText(expect.stringContaining("경로 복사"));
    await expect(menu).toHaveText(expect.stringContaining("태그 추가"));
  });
});
