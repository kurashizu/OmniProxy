// Admin HTTP polling helpers.

import { fetchAdmin } from "./ipc";
import type { ClientStats, ProxyStats, ProxyRoute } from "./schema";

export async function fetchProxyStats(
  baseUrl: string,
  signal?: AbortSignal,
): Promise<ProxyStats | null> {
  return fetchAdmin<ProxyStats>(baseUrl, "/stats", signal);
}

export async function fetchProxyRoutes(
  baseUrl: string,
  signal?: AbortSignal,
): Promise<ProxyRoute[] | null> {
  const r = await fetchAdmin<{ routes: ProxyRoute[] }>(
    baseUrl,
    "/routes",
    signal,
  );
  return r?.routes ?? null;
}

export async function fetchClientStats(
  baseUrl: string,
  signal?: AbortSignal,
): Promise<ClientStats | null> {
  return fetchAdmin<ClientStats>(baseUrl, "/stats", signal);
}

export async function fetchClientConnections(
  baseUrl: string,
  signal?: AbortSignal,
): Promise<ClientStats["connections"] | null> {
  const r = await fetchClientStats(baseUrl, signal);
  return r?.connections ?? null;
}
