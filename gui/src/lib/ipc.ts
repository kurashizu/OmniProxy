import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { ClientStats, GuiConfig, NodeConfig, ProxyState, ProxyStats, ProxyRoute } from "./schema";

export const ipc = {
  getGuiConfig: () => invoke<GuiConfig>("get_gui_config"),
  saveGuiConfig: (cfg: GuiConfig) => invoke<void>("save_gui_config", { cfg }),
  defaultConfigPath: () => invoke<string>("default_config_path"),
  openConfigDir: () => invoke<void>("open_config_dir"),
  upsertNode: (index: number | null, node: NodeConfig) =>
    invoke<number>("upsert_node", { index, node }),

  startProxy: () => invoke<ProxyState>("start_proxy"),
  stopProxy: () => invoke<ProxyState>("stop_proxy"),
  proxyStatus: () => invoke<ProxyState>("proxy_status"),
  proxyBinaryPath: () => invoke<string | null>("proxy_binary_path"),

  getProxyAdminUrl: () => invoke<string>("get_proxy_admin_url"),
  getClientAdminUrl: () => invoke<string>("get_client_admin_url"),

  isElevated: () => invoke<boolean>("is_elevated"),
  checkBinaryPresent: () => invoke<boolean>("check_binary_present"),
};

export async function openExternalUrl(url: string): Promise<void> {
  try {
    await openUrl(url);
  } catch {
    window.open(url, "_blank", "noopener");
  }
}

export async function fetchAdmin<T>(
  baseUrl: string, path: string, signal?: AbortSignal, timeoutMs = 1500,
): Promise<T | null> {
  const c = new AbortController();
  const t = setTimeout(() => c.abort(), timeoutMs);
  const s = signal ? composeSignals(signal, c.signal) : c.signal;
  try {
    const res = await fetch(`${baseUrl}${path}`, { signal: s });
    if (!res.ok) return null;
    return await res.json() as T;
  } catch { return null; }
  finally { clearTimeout(t); }
}

function composeSignals(a: AbortSignal, b: AbortSignal): AbortSignal {
  if (a.aborted || b.aborted) { const c = new AbortController(); c.abort(); return c.signal; }
  const c = new AbortController();
  const onAbort = () => c.abort();
  a.addEventListener("abort", onAbort, { once: true });
  b.addEventListener("abort", onAbort, { once: true });
  return c.signal;
}
