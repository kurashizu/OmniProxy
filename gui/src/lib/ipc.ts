// Type-safe wrapper around Tauri invoke commands.

import { invoke } from "@tauri-apps/api/core";
import { openUrl as tauriOpenUrl } from "@tauri-apps/plugin-opener";

import type {
  ClientStats,
  GuiConfig,
  NodeConfig,
  ProxyState,
  ProxyStats,
  ProxyRoute,
} from "./schema";

export const ipc = {
  // config
  getGuiConfig: () => invoke<GuiConfig>("get_gui_config"),
  saveGuiConfig: (cfg: GuiConfig) => invoke<void>("save_gui_config", { cfg }),
  defaultConfigPath: () => invoke<string>("default_config_path"),
  openConfigDir: () => invoke<void>("open_config_dir"),
  upsertNode: (index: number | null, node: NodeConfig) =>
    invoke<number>("upsert_node", { index, node }),

  // proxy lifecycle
  startProxy: () => invoke<ProxyState>("start_proxy"),
  stopProxy: () => invoke<ProxyState>("stop_proxy"),
  proxyStatus: () => invoke<ProxyState>("proxy_status"),
  proxyBinaryPath: () => invoke<string | null>("proxy_binary_path"),

  // admin URLs
  getProxyAdminUrl: () => invoke<string>("get_proxy_admin_url"),
  getClientAdminUrl: () => invoke<string>("get_client_admin_url"),

  // privilege / binary
  isElevated: () => invoke<boolean>("is_elevated"),
  checkBinaryPresent: () => invoke<boolean>("check_binary_present"),
};

// External URL opener (uses tauri-plugin-opener when in Tauri context, falls
// back to window.open in browser dev).
export async function openExternalUrl(url: string): Promise<void> {
  try {
    await tauriOpenUrl(url);
  } catch {
    if (typeof window !== "undefined") {
      window.open(url, "_blank", "noopener");
    }
  }
}

// Admin HTTP helpers — these are raw fetch calls (not Tauri commands). The
// proxy and client admin servers run on loopback.

export async function fetchAdmin<T>(
  baseUrl: string,
  path: string,
  signal?: AbortSignal,
  timeoutMs = 1500,
): Promise<T | null> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), timeoutMs);
  const composed = signal
    ? composeSignals(signal, controller.signal)
    : controller.signal;
  try {
    const res = await fetch(`${baseUrl}${path}`, { signal: composed });
    if (!res.ok) return null;
    return (await res.json()) as T;
  } catch {
    return null;
  } finally {
    clearTimeout(timeout);
  }
}

function composeSignals(a: AbortSignal, b: AbortSignal): AbortSignal {
  if (a.aborted || b.aborted) {
    const c = new AbortController();
    c.abort();
    return c.signal;
  }
  const c = new AbortController();
  const onAbort = () => c.abort();
  a.addEventListener("abort", onAbort, { once: true });
  b.addEventListener("abort", onAbort, { once: true });
  return c.signal;
}

export type { ClientStats, ProxyStats, ProxyRoute, ProxyState };
