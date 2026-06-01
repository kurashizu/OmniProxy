"use client";
import "@/styles/globals.css";
import { QueryClientProvider } from "@tanstack/react-query";
import { useEffect } from "react";
import { queryClient } from "@/store/queryClient";
import { useAppStore } from "@/store/appStore";
import { pickLocale } from "@/lib/i18n";
import { Sidebar } from "@/components/layout/Sidebar";
import { TopBar } from "@/components/layout/TopBar";
import { PrivilegeBanner } from "@/components/common/PrivilegeBanner";

export default function RootLayout({ children }: { children: React.ReactNode }) {
  const setLocale = useAppStore((s) => s.setLocale);
  useEffect(() => {
    setLocale(pickLocale());
  }, [setLocale]);

  return (
    <html lang="en" className="h-full">
      <body>
        <QueryClientProvider client={queryClient}>
          <div className="flex h-full">
            <Sidebar />
            <main className="flex-1 flex flex-col min-w-0">
              <TopBar />
              <PrivilegeBanner />
              <div className="flex-1 overflow-auto p-4 min-h-0">
                {children}
              </div>
            </main>
          </div>
        </QueryClientProvider>
      </body>
    </html>
  );
}
