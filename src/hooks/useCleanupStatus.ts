import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  commands,
  type CleanupEngineInfo,
  type CleanupStatus,
} from "@/bindings";

interface Progress {
  id: string;
  downloaded: number;
  total: number;
}

export type EngineState =
  | { kind: "ready" }
  | { kind: "loading" }
  | { kind: "downloaded" }
  | { kind: "remote" }
  | { kind: "downloading"; percent: number };

/** Cleanup status from the backend, kept live by its events. */
export const useCleanupStatus = () => {
  const [status, setStatus] = useState<CleanupStatus | null>(null);
  const [progress, setProgress] = useState<Progress | null>(null);

  const refresh = useCallback(() => {
    commands.getCleanupStatus().then(setStatus);
  }, []);

  useEffect(() => {
    refresh();
    const unlisten = [
      listen("cleanup-status-changed", refresh),
      listen<Progress>("cleanup-download-progress", (e) =>
        setProgress(e.payload),
      ),
      listen("cleanup-download-complete", () => {
        setProgress(null);
        refresh();
      }),
      listen("cleanup-download-failed", () => {
        setProgress(null);
        refresh();
      }),
    ];
    return () => {
      unlisten.forEach((p) => p.then((off) => off()));
    };
  }, [refresh]);

  /** Where one engine stands: in use, loading, on disk, downloading, or not. */
  const stateOf = (engine: CleanupEngineInfo): EngineState => {
    const live = progress?.id === engine.id ? progress : null;
    if (live || engine.downloading != null) {
      const done = live?.downloaded ?? engine.downloading ?? 0;
      const total = live?.total ?? engine.size_mb * 1_000_000;
      return {
        kind: "downloading",
        percent: Math.min(99, Math.round((100 * done) / Math.max(total, 1))),
      };
    }
    if (!engine.downloaded) return { kind: "remote" };
    if (status?.enabled && status.engine === engine.id)
      return status.ready ? { kind: "ready" } : { kind: "loading" };
    return { kind: "downloaded" };
  };

  return { status, refresh, stateOf };
};
