"use client";
import { useElevated } from "@/hooks/useElevated";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";

export function PrivilegeBanner() {
  const elevated = useElevated();
  const locale = useAppStore((s) => s.locale);
  if (elevated !== false) return null;
  return (
    <div
      className="flex items-center gap-3 border-b border-[#f59e0b]/30 bg-[#f59e0b]/10 px-6 py-2 text-sm text-[#f59e0b]"
      role="alert"
    >
      <span className="text-base">⚠</span>
      <div className="flex flex-col">
        <span className="font-medium">{t("privilege.banner.title", locale)}</span>
        <span className="text-xs text-[#9ca3af]">
          {t("privilege.banner.body", locale)}
        </span>
      </div>
    </div>
  );
}
