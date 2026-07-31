/// <reference types="mocha" />
import { expect } from "@wdio/globals";

describe("EveryFile independent workspace panels", () => {
  it("toggles the left and right panels independently and persists the choices", async () => {
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
  });
});
