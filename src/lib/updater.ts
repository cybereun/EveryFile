import { isTauri } from "@tauri-apps/api/core";

export interface UpdateProgress {
  phase: "starting" | "downloading" | "verifying" | "installing" | "restarting";
  downloadedBytes: number;
  contentLength?: number;
  percent?: number;
}

export interface AvailableUpdate {
  currentVersion: string;
  version: string;
  date?: string;
  notes: string;
  install: (onProgress?: (progress: UpdateProgress) => void) => Promise<void>;
}

/**
 * Checks the signed Tauri updater manifest. Browser/Vite previews intentionally
 * return null so tests and the local UI never make a network request.
 */
export async function checkForUpdate(): Promise<AvailableUpdate | null> {
  if (!isTauri()) return null;

  const { check } = await import("@tauri-apps/plugin-updater");
  const update = await check({ timeout: 12_000 });
  if (!update) return null;

  return {
    currentVersion: update.currentVersion,
    version: update.version,
    date: update.date,
    notes: update.body?.trim() || "새 버전의 안정성과 기능이 개선되었습니다.",
    install: async (onProgress) => {
      let downloadedBytes = 0;
      let contentLength: number | undefined;
      await update.download((event) => {
        if (event.event === "Started") {
          downloadedBytes = 0;
          contentLength = event.data.contentLength;
          onProgress?.({
            phase: "starting",
            downloadedBytes: 0,
            contentLength: event.data.contentLength,
            percent: contentLength ? 0 : undefined,
          });
          return;
        }
        if (event.event === "Progress") {
          downloadedBytes += event.data.chunkLength;
          onProgress?.({
            phase: "downloading",
            downloadedBytes,
            contentLength,
            percent: contentLength
              ? Math.min(100, Math.round((downloadedBytes / contentLength) * 100))
              : undefined,
          });
          return;
        }
        onProgress?.({
          phase: "verifying",
          downloadedBytes,
          contentLength,
        });
      });
      // download() resolves only after signature verification. Do not quit the
      // running app or report installation complete on a download-finished event.
      onProgress?.({ phase: "installing", downloadedBytes, contentLength });
      await update.install();
      // Windows exits inside install(); the passive NSIS /R path restarts it
      // after successful installation. Other platforms return here.
      onProgress?.({ phase: "restarting", downloadedBytes, contentLength });
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    },
  };
}
