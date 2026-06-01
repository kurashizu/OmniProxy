import { create } from "zustand";
import type { Locale, LogEntry } from "@/lib/schema";

export type Theme = "dark" | "light";

interface AppState {
  locale: Locale;
  setLocale: (l: Locale) => void;
  logBuffer: LogEntry[];
  appendLog: (entries: LogEntry[]) => void;
  clearLogs: () => void;
  logsPaused: boolean;
  setLogsPaused: (p: boolean) => void;
  dropped: number;
  addDropped: (n: number) => void;
  connectionFilter: ConnectionFilter;
  setConnectionFilter: (f: Partial<ConnectionFilter>) => void;
  // Connection table UI state
  connsPaused: boolean;
  setConnsPaused: (p: boolean) => void;
  // Routes table UI state
  routesPaused: boolean;
  setRoutesPaused: (p: boolean) => void;
}

export interface ConnectionFilter {
  search: string;
  protocol: { TCP: boolean; UDP: boolean; ICMP: boolean };
  sort: "durationDesc" | "durationAsc" | "idDesc" | "idAsc";
}

const LOG_CAPACITY = 5000;

export const useAppStore = create<AppState>((set) => ({
  locale: "en",
  setLocale: (locale) => {
    set({ locale });
    try {
      localStorage.setItem("omniproxy.locale", locale);
    } catch {}
  },
  logBuffer: [],
  appendLog: (entries) =>
    set((s) => {
      let next = s.logBuffer.concat(entries);
      let dropped = s.dropped;
      if (next.length > LOG_CAPACITY) {
        dropped += next.length - LOG_CAPACITY;
        next = next.slice(next.length - LOG_CAPACITY);
      }
      return { logBuffer: next, dropped };
    }),
  clearLogs: () => set({ logBuffer: [], dropped: 0 }),
  logsPaused: false,
  setLogsPaused: (logsPaused) => set({ logsPaused }),
  dropped: 0,
  addDropped: (n) => set((s) => ({ dropped: s.dropped + n })),
  connectionFilter: {
    search: "",
    protocol: { TCP: true, UDP: true, ICMP: true },
    sort: "durationDesc",
  },
  setConnectionFilter: (f) =>
    set((s) => ({ connectionFilter: { ...s.connectionFilter, ...f } })),
  connsPaused: false,
  setConnsPaused: (connsPaused) => set({ connsPaused }),
  routesPaused: false,
  setRoutesPaused: (routesPaused) => set({ routesPaused }),
}));

export const LOG_CAPACITY_EXPORT = LOG_CAPACITY;
