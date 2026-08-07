import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useState } from "react";
import type { CommandStatusMessage } from "../components/CommandStatus";
import {
  listFolders,
  openFolderLocation,
  registerFolder,
  removeFolder,
  startIndexing,
  getIndexStatus,
} from "../lib/ipc";
import type { FolderRecord, IndexStatus } from "../lib/types";
import { App } from "./App";

function errorText(error: unknown) {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  if (typeof error === "object" && error !== null) {
    const value = error as { code?: unknown; message?: unknown };
    const message = typeof value.message === "string" ? value.message : "알 수 없는 오류가 발생했습니다.";
    return typeof value.code === "string" ? `${message} (${value.code})` : message;
  }
  return "알 수 없는 오류가 발생했습니다.";
}

export function DesktopApp() {
  const [folders, setFolders] = useState<FolderRecord[]>([]);
  const [queueState, setQueueState] =
    useState<"idle" | "indexing" | "paused" | "error">("idle");
  const [status, setStatus] = useState<CommandStatusMessage | null>(null);
  // Only jobs explicitly started from this window should open a completion
  // report. File-system watcher jobs are intentionally silent.
  const [reportJobIds, setReportJobIds] = useState<ReadonlySet<string>>(
    () => new Set(),
  );

  const refreshFolders = useCallback(async () => {
    setFolders(await listFolders());
  }, []);

  useEffect(() => {
    void refreshFolders().catch((error) => {
      setStatus({
        kind: "error",
        text: `등록 폴더를 불러오지 못했습니다: ${errorText(error)}`,
      });
    });
  }, [refreshFolders]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<IndexStatus>("index-status://changed", (event) => {
      const next = event.payload;
      if (next.silent) return;
      if (next.state === "failed") setQueueState("error");
      else if (next.state === "paused") setQueueState("paused");
      else if (["completed", "cancelled"].includes(next.state)) {
        setQueueState("idle");
      } else {
        setQueueState("indexing");
      }
      if (["completed", "cancelled", "failed"].includes(next.state)) {
        void refreshFolders().catch(() => undefined);
      }
    })
      .then((remove) => {
        if (disposed) remove();
        else unlisten = remove;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [refreshFolders]);

  const indexedDocumentCount = useMemo(
    () => folders.reduce((total, folder) => total + folder.documentCount, 0),
    [folders],
  );

  const addFolder = async () => {
    setStatus({ kind: "info", text: "폴더를 선택하는 중입니다…" });
    try {
      const registration = await registerFolder();
      if (!registration) {
        setStatus(null);
        return;
      }
      const { folder, jobId } = registration;
      await refreshFolders();
      const currentStatus = await getIndexStatus(jobId);
      if (currentStatus.state === "failed") setQueueState("error");
      else if (["completed", "cancelled"].includes(currentStatus.state)) {
        setQueueState("idle");
      } else {
        setQueueState("indexing");
      }
      setStatus({
        kind: "success",
        text: `${folder.displayName} 폴더를 등록했습니다. 색인을 시작합니다.`,
      });
      setReportJobIds((current) => new Set(current).add(jobId));
    } catch (error) {
      setQueueState("error");
      setStatus({
        kind: "error",
        text: `폴더를 등록하지 못했습니다: ${errorText(error)}`,
      });
    }
  };

  const deleteFolder = async (folderId: string) => {
    setStatus({ kind: "info", text: "등록 폴더와 색인 데이터를 삭제하는 중입니다…" });
    try {
      await removeFolder(folderId);
      await refreshFolders();
      setStatus({ kind: "success", text: "등록 폴더를 제거했습니다." });
    } catch (error) {
      setStatus({
        kind: "error",
        text: `폴더를 제거하지 못했습니다: ${errorText(error)}`,
      });
    }
  };

  const reindexFolder = async (folderId: string) => {
    try {
      setQueueState("indexing");
      const jobId = await startIndexing(folderId);
      setReportJobIds((current) => new Set(current).add(jobId));
      setStatus({ kind: "info", text: "폴더를 다시 색인합니다." });
    } catch (error) {
      setQueueState("error");
      setStatus({ kind: "error", text: `재인덱싱을 시작하지 못했습니다: ${errorText(error)}` });
    }
  };

  const showFolder = async (folderId: string) => {
    try {
      await openFolderLocation(folderId);
    } catch (error) {
      setStatus({ kind: "error", text: `폴더를 열지 못했습니다: ${errorText(error)}` });
    }
  };

  return (
    <App
      folders={folders}
      indexedDocumentCount={indexedDocumentCount}
      queueState={queueState}
      reportJobIds={reportJobIds}
      commandStatus={status}
      onDismissCommandStatus={() => setStatus(null)}
      onAddFolder={() => void addFolder()}
      onRemoveFolder={(folderId) => void deleteFolder(folderId)}
      onOpenFolder={(folderId) => void showFolder(folderId)}
      onReindexFolder={(folderId) => void reindexFolder(folderId)}
    />
  );
}
