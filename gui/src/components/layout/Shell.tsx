"use client";
import { QueryClientProvider } from "@tanstack/react-query";
import { useEffect } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { queryClient } from "@/store/queryClient";
import { useAppStore } from "@/store/appStore";
import { pickLocale } from "@/lib/i18n";
import { TopBar } from "./TopBar";
import { Sidebar } from "./Sidebar";
import { PrivilegeBanner } from "@/components/common/PrivilegeBanner";
import { ErrorBanner } from "@/components/common/ErrorBanner";

export function Shell({ children }: { children: React.ReactNode }) {
    const setLocale = useAppStore((s) => s.setLocale);
    const setLastProxyError = useAppStore((s) => s.setLastProxyError);
    const isDev = process.env.NODE_ENV !== "production";

    useEffect(() => {
        setLocale(pickLocale());
    }, [setLocale]);

    // Listen for proxy-error banners.
    useEffect(() => {
        let unlisten: UnlistenFn | undefined;
        let cancelled = false;
        (async () => {
            try {
                const u = await listen<{ ts_ms: number; line: string }>(
                    "proxy-error",
                    (e) => setLastProxyError(e.payload),
                );
                if (cancelled) u();
                else unlisten = u;
            } catch {}
        })();
        return () => {
            cancelled = true;
            unlisten?.();
        };
    }, [setLastProxyError]);

    const app = (
        <div className="flex flex-col h-full bg-surface">
            <TopBar />
            <div className="flex flex-row flex-1 min-h-0">
                <Sidebar />
                <main className="flex-1 flex flex-col min-w-0 min-h-0">
                    <PrivilegeBanner />
                    <ErrorBanner />
                    <div className="flex-1 overflow-auto p-3 min-h-0">
                        {children}
                    </div>
                </main>
            </div>
        </div>
    );

    return (
        <QueryClientProvider client={queryClient}>
            {isDev ? (
                <div className="flex h-screen w-screen items-center justify-center overflow-auto bg-[#090b10] p-6">
                    <div className="h-[700px] w-[900px] overflow-hidden rounded-[14px] border border-border bg-surface shadow-2xl shadow-black/40">
                        {app}
                    </div>
                </div>
            ) : (
                app
            )}
        </QueryClientProvider>
    );
}
