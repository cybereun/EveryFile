import { isTauri } from "@tauri-apps/api/core";

export interface UpdateProgress {
  phase: "starting" | "downloading" | "finished";
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
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          onProgress?.({
            phase: "starting",
            downloadedBytes: 0,
            contentLength: event.data.contentLength,
            percent: 0,
          });
          return;
        }
        if (event.event === "Progress") {
          downloadedBytes += event.data.chunkLength;
          onProgress?.({
            phase: "downloading",
            downloadedBytes,
            percent: undefined,
          });
          return;
        }
        onProgress?.({
          phase: "finished",
          downloadedBytes,
          percent: 100,
        });
      });

      // The passive Windows installer normally restarts the process itself.
      // Relaunch is still useful for platforms/install modes that return here.
      try {
        const { relaunch } = await import("@tauri-apps/plugin-process");
        await relaunch();
      } catch {
        // If the installer already closed the process, this code is unreachable.
        // A failed relaunch must not turn a successful installation into an error.
      }
    },
  };
}
