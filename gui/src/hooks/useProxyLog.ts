import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useRef } from "react";
import { useAppStore } from "@/store/appStore";
import type { LogEntry } from "@/lib/schema";

/**
 * Subscribe to the `proxy-log` Tauri event and push entries into the
 * app-store ring buffer (only when `logsPaused` is false).
 */
export function useProxyLog() {
  const appendLog = useAppStore((s) => s.appendLog);
  const clearLogs = useAppStore((s) => s.clearLogs);
  const dropped = useAppStore.getState; // subscribe to "logsPaused" in the
  // component using the hook instead.

  const buffer = useRef<LogEntry[]>([]);
  const timer = useRef<number | null>(null);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    (async () => {
      try {
        unlisten = await listen<LogEntry>("proxy-log", (e) => {
          buffer.current.push(e.payload);
        });
      } catch {
        // ignore
      }
    })();
    const flushInterval = window.setInterval(() => {
      if (buffer.current.length > 0) {
        const drained = buffer.current.splice(0, buffer.current.length);
        appendLog(drained);
      }
    }, 200);

    return () => {
      window.clearInterval(flushInterval);
      unlisten?.();
      if (timer.current) window.clearInterval(timer.current);
    };
  }, [appendLog, dropped]);

  return { clearLogs };
}
