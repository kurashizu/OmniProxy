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

  // Stats are fetched on the Rust side via Tauri commands. We can't
  // use the webview's fetch() for this — WebView2 has been observed
  // to silently fail on loopback requests in some configurations,
  // leaving the UI with permanent placeholder state.
  proxyStats: () => invoke<ProxyStats | null>("proxy_stats"),
  clientStats: () => invoke<ClientStats | null>("client_stats"),
  proxyRoutes: () => invoke<ProxyRoute[] | null>("proxy_routes"),

  isElevated: () => invoke<boolean>("is_elevated"),
  checkBinaryPresent: () => invoke<boolean>("check_binary_present"),

  logDir: () => invoke<string>("log_dir"),
  openLogDir: () => invoke<void>("open_log_dir"),
};

export async function openExternalUrl(url: string): Promise<void> {
  try {
    await openUrl(url);
  } catch {
    window.open(url, "_blank", "noopener");
  }
}
