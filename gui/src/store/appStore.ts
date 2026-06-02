import { create } from "zustand";
import type { Locale, LogEntry } from "@/lib/schema";

export interface ConnectionFilter {
  search: string;
  protocol: { TCP: boolean; UDP: boolean; ICMP: boolean };
  sort: "durationDesc" | "durationAsc" | "idDesc" | "idAsc";
}

interface AppState {
  locale: Locale;
  setLocale: (l: Locale) => void;
  logBuffer: LogEntry[];
  appendLog: (entries: LogEntry[]) => void;
  clearLogs: () => void;
  logsPaused: boolean;
  setLogsPaused: (p: boolean) => void;
  dropped: number;
  connectionFilter: ConnectionFilter;
  setConnectionFilter: (f: Partial<ConnectionFilter>) => void;
  connsPaused: boolean;
  setConnsPaused: (p: boolean) => void;
  routesPaused: boolean;
  setRoutesPaused: (p: boolean) => void;
}

const LOG_CAP = 5000;

export const useAppStore = create<AppState>((set) => ({
  locale: "en",
  setLocale: (locale) => {
    set({ locale });
    try { localStorage.setItem("omniproxy.locale", locale); } catch {}
  },
  logBuffer: [],
  appendLog: (entries) => set((s) => {
    let next = s.logBuffer.concat(entries);
    let dropped = 0;
    if (next.length > LOG_CAP) {
      dropped = next.length - LOG_CAP;
      next = next.slice(next.length - LOG_CAP);
    }
    return { logBuffer: next, dropped: s.dropped + dropped };
  }),
  clearLogs: () => set({ logBuffer: [], dropped: 0 }),
  logsPaused: false,
  setLogsPaused: (logsPaused) => set({ logsPaused }),
  dropped: 0,
  connectionFilter: {
    search: "",
    protocol: { TCP: true, UDP: true, ICMP: true },
    sort: "durationDesc",
  },
  setConnectionFilter: (f) => set((s) => ({ connectionFilter: { ...s.connectionFilter, ...f } })),
  connsPaused: false,
  setConnsPaused: (connsPaused) => set({ connsPaused }),
  routesPaused: false,
  setRoutesPaused: (routesPaused) => set({ routesPaused }),
}));
