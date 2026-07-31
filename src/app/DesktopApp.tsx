import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useState } from "react";
import type { CommandStatusMessage } from "../components/CommandStatus";
import {
  listFolders,
  registerFolder,
  removeFolder,
  startIndexing,
} from "../lib/ipc";
import type { FolderRecord, IndexStatus } from "../lib/types";
import { App } from "./App";

function errorText(error: unknown) {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "알 수 없는 오류가 발생했습니다.";
}

export function DesktopApp() {
  const [folders, setFolders] = useState<FolderRecord[]>([]);
  const [queueState, setQueueState] =
    useState<"idle" | "indexing" | "paused" | "error">("idle");
  const [status, setStatus] = useState<CommandStatusMessage | null>(null);

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
      const folder = await registerFolder();
      if (!folder) {
        setStatus(null);
        return;
      }
      await refreshFolders();
      setQueueState("indexing");
      setStatus({
        kind: "success",
        text: `${folder.displayName} 폴더를 등록했습니다. 색인을 시작합니다.`,
      });
      await startIndexing(folder.id);
    } catch (error) {
      setQueueState("error");
      setStatus({
        kind: "error",
        text: `폴더를 등록하지 못했습니다: ${errorText(error)}`,
      });
    }
  };

  const deleteFolder = async (folderId: string) => {
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

  return (
    <App
      folders={folders}
      indexedDocumentCount={indexedDocumentCount}
      queueState={queueState}
      commandStatus={status}
      onDismissCommandStatus={() => setStatus(null)}
      onAddFolder={() => void addFolder()}
      onRemoveFolder={(folderId) => void deleteFolder(folderId)}
    />
  );
}
