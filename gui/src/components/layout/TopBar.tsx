"use client";
import Image from "next/image";
import { useEffect, useRef } from "react";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";

type TauriWin = { minimize(): Promise<void>; close(): Promise<void> };

export function TopBar() {
  const locale = useAppStore((s) => s.locale);
  const winRef = useRef<TauriWin | null>(null);

  useEffect(() => {
    import("@tauri-apps/api/window")
      .then((m) => { winRef.current = m.getCurrentWindow() as TauriWin; })
      .catch(() => {});
  }, []);

  return (
    <header data-tauri-drag-region className="flex items-center justify-between bg-surface h-9 flex-none select-none">
      <div className="flex items-center gap-2 pl-3">
        <Image src="/icon.png" alt="OmniProxy" width={22} height={22} className="rounded" />
        <span className="text-text text-[13px] font-semibold tracking-wide">
          {t("app.name", locale)}
        </span>
      </div>
      <div className="flex">
        <button
          onClick={() => winRef.current?.minimize()}
          className="flex items-center justify-center w-9 h-9 text-muted hover:text-text hover:bg-border transition-colors"
        >
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
            <path d="M2 6h8" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
          </svg>
        </button>
        <button
          onClick={() => winRef.current?.close()}
          className="flex items-center justify-center w-9 h-9 text-muted hover:text-white hover:bg-danger transition-colors"
        >
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
            <path d="M3 3l6 6M9 3l-6 6" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
          </svg>
        </button>
      </div>
    </header>
  );
}
