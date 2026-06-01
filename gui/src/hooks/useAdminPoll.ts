import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ipc, fetchAdmin } from "@/lib/ipc";
import type { ClientStats, ProxyStats, ProxyRoute } from "@/lib/schema";

/**
 * Generic admin poll hook. Returns the latest successful response or null
 * while loading / on error.
 */
function useAdminPoll<T>(
  baseUrl: string | null,
  path: string,
  intervalMs: number,
  enabled: boolean,
) {
  return useQuery<T | null>({
    queryKey: ["admin", baseUrl, path, intervalMs],
    queryFn: ({ signal }) => fetchAdmin<T>(baseUrl ?? "", path, signal),
    enabled: enabled && !!baseUrl,
    refetchInterval: enabled ? intervalMs : false,
    refetchIntervalInBackground: false,
    staleTime: 0,
  });
}

export function useProxyStats(running: boolean) {
  const [baseUrl, setBaseUrl] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    ipc
      .getProxyAdminUrl()
      .then(setBaseUrl)
      .catch(() => setBaseUrl(null));
    return () => {
      cancelled = true;
      void cancelled;
    };
  }, []);
  return useAdminPoll<ProxyStats>(baseUrl, "/stats", 1000, running);
}

export function useProxyRoutes(running: boolean) {
  const [baseUrl, setBaseUrl] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    ipc
      .getProxyAdminUrl()
      .then(setBaseUrl)
      .catch(() => setBaseUrl(null));
    return () => {
      cancelled = true;
      void cancelled;
    };
  }, []);
  return useQuery<ProxyRoute[] | null>({
    queryKey: ["admin", baseUrl, "/routes", 5000],
    queryFn: ({ signal }) =>
      fetchAdmin<{ routes: ProxyRoute[] }>(baseUrl ?? "", "/routes", signal).then(
        (r) => r?.routes ?? null,
      ),
    enabled: running && !!baseUrl,
    refetchInterval: running ? 5000 : false,
    refetchIntervalInBackground: false,
  });
}

export function useClientStats(running: boolean) {
  const [baseUrl, setBaseUrl] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    ipc
      .getClientAdminUrl()
      .then(setBaseUrl)
      .catch(() => setBaseUrl(null));
    return () => {
      cancelled = true;
      void cancelled;
    };
  }, []);
  return useAdminPoll<ClientStats>(baseUrl, "/stats", 1000, running);
}

export function useClientConnections(running: boolean, intervalMs = 1000) {
  const [baseUrl, setBaseUrl] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    ipc
      .getClientAdminUrl()
      .then(setBaseUrl)
      .catch(() => setBaseUrl(null));
    return () => {
      cancelled = true;
      void cancelled;
    };
  }, []);
  return useAdminPoll<ClientStats>(baseUrl, "/stats", intervalMs, running);
}
