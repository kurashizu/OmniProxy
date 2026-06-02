"use client";
import { QueryClientProvider } from "@tanstack/react-query";
import { useEffect } from "react";
import { queryClient } from "@/store/queryClient";
import { useAppStore } from "@/store/appStore";
import { pickLocale } from "@/lib/i18n";
import { TopBar } from "./TopBar";
import { Sidebar } from "./Sidebar";
import { PrivilegeBanner } from "@/components/common/PrivilegeBanner";

export function Shell({ children }: { children: React.ReactNode }) {
  const setLocale = useAppStore((s) => s.setLocale);
  const isDev = process.env.NODE_ENV !== "production";
  useEffect(() => { setLocale(pickLocale()); }, [setLocale]);

  const app = (
    <div className="flex flex-col h-full">
      <TopBar />
      <div className="flex flex-row flex-1 min-h-0">
        <Sidebar />
        <main className="flex-1 flex flex-col min-w-0 min-h-0">
          <PrivilegeBanner />
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
          <div className="h-[650px] w-[700px] overflow-hidden rounded-[14px] border border-border bg-surface shadow-2xl shadow-black/40">
            {app}
          </div>
        </div>
      ) : app}
    </QueryClientProvider>
  );
}
