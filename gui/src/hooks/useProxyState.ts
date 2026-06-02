import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { ipc } from "@/lib/ipc";
import type { ProxyState } from "@/lib/schema";

export function useProxyState(): { state: ProxyState; refresh: () => Promise<void> } {
  const [state, setState] = useState<ProxyState>({
    state: "stopped", pid: 0, exit_code: null, message: null,
  });

  const refresh = async () => {
    try { setState(await ipc.proxyStatus()); } catch {}
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
        if (cancelled) u(); else unlisten = u;
      } catch {}
    })();
    return () => { cancelled = true; unlisten?.(); };
  }, []);

  return { state, refresh };
}
