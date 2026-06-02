"use client";
import { useAppStore } from "@/store/appStore";
import type { Locale } from "@/lib/schema";

export function LanguageSwitcher() {
  const locale = useAppStore((s) => s.locale);
  const setLocale = useAppStore((s) => s.setLocale);
  return (
    <select
      value={locale}
      onChange={(e) => setLocale(e.target.value as Locale)}
      className="text-sm"
    >
      <option value="en">EN</option>
      <option value="zh">中文</option>
    </select>
  );
}
