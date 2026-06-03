import { create } from "zustand";
import type { Locale } from "@/lib/schema";

export interface ConnectionFilter {
    search: string;
    protocol: { TCP: boolean; UDP: boolean; ICMP: boolean };
    sort: "durationDesc" | "durationAsc" | "idDesc" | "idAsc";
}

interface AppState {
    locale: Locale;
    setLocale: (l: Locale) => void;
    /** Last "interesting" stderr line we detected via `proxy-error`. */
    lastProxyError: { ts_ms: number; line: string } | null;
    setLastProxyError: (e: { ts_ms: number; line: string } | null) => void;
    connectionFilter: ConnectionFilter;
    setConnectionFilter: (f: Partial<ConnectionFilter>) => void;
    connsPaused: boolean;
    setConnsPaused: (p: boolean) => void;
    routesPaused: boolean;
    setRoutesPaused: (p: boolean) => void;
}

export const useAppStore = create<AppState>((set) => ({
    locale: "en",
    setLocale: (locale) => {
        set({ locale });
        try {
            localStorage.setItem("omniproxy.locale", locale);
        } catch {}
    },
    lastProxyError: null,
    setLastProxyError: (e) => set({ lastProxyError: e }),
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
