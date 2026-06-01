"use client";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";

export function LanguageSwitcher() {
  const locale = useAppStore((s) => s.locale);
  const setLocale = useAppStore((s) => s.setLocale);
  const next = locale === "en" ? "zh" : "en";
  return (
    <button
      onClick={() => setLocale(next)}
      className="rounded-md border border-[#252934] bg-[#171a21] px-2 py-1 text-xs text-[#9ca3af] hover:text-[#e5e7eb] hover:border-[#3b82f6] transition-colors"
      title="Toggle language"
    >
      {locale === "en" ? "🌐 EN ⇄ 中" : "🌐 中 ⇄ EN"}
    </button>
  );
}
