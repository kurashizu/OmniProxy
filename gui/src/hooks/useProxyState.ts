import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { ipc } from "@/lib/ipc";
import type { ProxyState } from "@/lib/schema";

/**
 * Subscribe to the `proxy-state` Tauri event and seed the initial value
 * via `proxy_status` on mount.
 */
export function useProxyState(): {
  state: ProxyState;
  refresh: () => Promise<void>;
} {
  const [state, setState] = useState<ProxyState>({
    state: "stopped",
    pid: 0,
    exit_code: null,
    message: null,
  });

  const refresh = async () => {
    try {
      const s = await ipc.proxyStatus();
      setState(s);
    } catch {
      // ignore
    }
  };

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;
    (async () => {
      try {
        await refresh();
        const u = await listen<ProxyState>("proxy-state", (e) => {
          if (!cancelled) setState(e.payload);
        });
        if (cancelled) {
          u();
        } else {
          unlisten = u;
        }
      } catch {
        // Tauri not available (browser dev)
      }
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return { state, refresh };
}
