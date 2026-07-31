/// <reference types="mocha" />
import { expect } from "@wdio/globals";

describe("EveryFile packaged shell", () => {
  it("starts with a connected folder action", async () => {
    const heading = await $("h1");
    if (!(await heading.isDisplayed())) {
      throw new Error(`EveryFile shell was not rendered: ${await browser.getPageSource()}`);
    }
    await expect(heading).toBeDisplayed();
    const source = await browser.getPageSource();
    if (!source.includes("icon-button--accent")) {
      throw new Error(`Connected folder action was not rendered: ${source}`);
    }
    const addFolder = await $(".icon-button--accent");
    await expect(addFolder).toBeEnabled();
  });
});
