/// <reference types="mocha" />
import { createServer, type Server } from "node:http";
import { expect } from "@wdio/globals";

describe("EveryFile explicit AI selection", () => {
  let ollama: Server;

  before(async () => {
    ollama = createServer((request, response) => {
      if (request.url !== "/api/chat") {
        response.writeHead(404).end();
        return;
      }
      response.writeHead(200, { "content-type": "application/x-ndjson" });
      response.end(
        [
          JSON.stringify({ message: { content: "로컬 " }, done: false }),
          JSON.stringify({ message: { content: "요약 완료" }, done: true }),
        ].join("\n"),
      );
    });
    await new Promise<void>((resolve, reject) => {
      ollama.once("error", reject);
      ollama.listen(14567, "127.0.0.1", resolve);
    });
  });

  after(async () => {
    await new Promise<void>((resolve) => ollama.close(() => resolve()));
  });

  it("keeps AI hidden while disabled, requires remote consent, and runs selected Ollama", async () => {
    const search = await $("input[aria-label='검색어']");
    await search.setValue("everyfile-local-content-phrase-7291");
    const result = await $(".search-result-row");
    await result.waitForDisplayed({ timeout: 60_000 });
    await result.click();
    await expect(await $("button=AI 요약")).not.toBeExisting();

    await $("button[aria-label='설정 / Settings']").click();
    await $("button=AI").click();
    const activation = await $("input[aria-label='AI 기능 활성화']");
    await activation.click();
    const provider = await $("select[aria-label='LLM Provider']");
    await browser.execute(() => {
      const select = document.querySelector<HTMLSelectElement>(
        "select[aria-label='LLM Provider']",
      );
      if (!select) throw new Error("LLM provider select is unavailable");
      select.value = "openai";
      select.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await expect(provider).toHaveValue("openai");
    await $("button=저장").click();
    await expect(await $(".dialog-footer [role='status']")).toHaveText(
      expect.stringContaining("저장했습니다"),
    );
    const persisted = await browser.tauri.execute(
      ({ core }) => core.invoke<{ aiProvider: string }>("get_settings"),
    );
    expect(persisted.aiProvider).toBe("openai");
    await $("button[aria-label='설정 닫기']").click();

    await $("button=AI 요약").click();
    await expect(await $(".ai-consent")).toBeDisplayed();
    const execute = await $("button=실행");
    await expect(execute).not.toBeEnabled();
    await $(".ai-consent input").click();
    await expect(execute).toBeEnabled();
    await $("button[aria-label='AI 패널 닫기']").click();

    await $("button[aria-label='설정 / Settings']").click();
    await $("button=AI").click();
    await browser.execute(() => {
      const select = document.querySelector<HTMLSelectElement>(
        "select[aria-label='LLM Provider']",
      );
      if (!select) throw new Error("LLM provider select is unavailable");
      select.value = "ollama";
      select.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await (await $("label*=Base URL")).$("input").setValue(
      "http://127.0.0.1:14567",
    );
    await $("button=저장").click();
    await expect(await $(".dialog-footer [role='status']")).toHaveText(
      expect.stringContaining("저장했습니다"),
    );
    await $("button[aria-label='설정 닫기']").click();
    await $("button=AI 요약").click();
    await $("button=실행").click();
    await expect(await $(".ai-answer")).toHaveText("로컬 요약 완료");
  });
});
