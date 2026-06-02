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
  useEffect(() => { setLocale(pickLocale()); }, [setLocale]);

  return (
    <QueryClientProvider client={queryClient}>
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
    </QueryClientProvider>
  );
}
