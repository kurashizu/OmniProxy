"use client";
import Image from "next/image";
import { useAppStore } from "@/store/appStore";
import { LanguageSwitcher } from "@/components/common/LanguageSwitcher";
import { t } from "@/lib/i18n";
import { openExternalUrl } from "@/lib/ipc";

export function TopBar() {
  const locale = useAppStore((s) => s.locale);
  return (
    <header className="flex items-center justify-between border-b border-[#252934] bg-[#0f1115] px-4 h-12 flex-none">
      <div className="flex items-center gap-2">
        <Image
          src="/icon.png"
          alt="OmniProxy"
          width={28}
          height={28}
          className="rounded"
        />
        <span className="text-[#e5e7eb] font-semibold tracking-wide">
          {t("app.name", locale)}
        </span>
      </div>
      <div className="flex items-center gap-2">
        <LanguageSwitcher />
        <button
          onClick={() =>
            openExternalUrl("https://github.com/kurashizu/OmniProxy")
          }
          className="rounded-md border border-[#252934] bg-[#171a21] px-2 py-1 text-xs text-[#9ca3af] hover:text-[#e5e7eb] hover:border-[#3b82f6] transition-colors"
          title="Help"
        >
          ?
        </button>
      </div>
    </header>
  );
}
