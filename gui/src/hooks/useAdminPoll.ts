import { useQuery } from "@tanstack/react-query";
import { ipc } from "@/lib/ipc";
import type { ClientStats, ProxyStats, ProxyRoute } from "@/lib/schema";

/**
 * Polls the proxy / client admin `/stats` endpoint via a Rust-side
 * Tauri command. We don't use the webview's `fetch()` because
 * WebView2 has been observed to silently fail on loopback requests
 * in some configurations, leaving the UI stuck on placeholders.
 *
 * The Rust command short-circuits to `None` when no proxy is
 * running, so polling an idle GUI has no cost beyond an IPC roundtrip.
 */
function useAdminPoll<T>(
  command: () => Promise<T | null>,
  key: string,
  intervalMs: number,
  enabled: boolean,
) {
  return useQuery<T | null>({
    queryKey: ["admin", key, intervalMs],
    queryFn: () => command(),
    enabled,
    refetchInterval: enabled ? intervalMs : false,
    refetchIntervalInBackground: false,
    staleTime: 0,
  });
}

export function useProxyStats(running: boolean) {
  return useAdminPoll<ProxyStats>(ipc.proxyStats, "proxy", 1000, running);
}

export function useProxyRoutes(running: boolean) {
  return useAdminPoll<ProxyRoute[]>(ipc.proxyRoutes, "proxy-routes", 5000, running);
}

export function useClientStats(running: boolean) {
  return useAdminPoll<ClientStats>(ipc.clientStats, "client", 1000, running);
}

export function useClientConnections(running: boolean, intervalMs = 1000) {
  return useAdminPoll<ClientStats>(ipc.clientStats, "client-conns", intervalMs, running);
}
