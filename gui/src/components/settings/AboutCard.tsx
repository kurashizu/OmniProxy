"use client";
import { Card } from "@/components/common/Card";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";
import { openExternalUrl } from "@/lib/ipc";
import { useAppInfo } from "@/hooks/useAppInfo";

export function AboutCard() {
  const locale = useAppStore((s) => s.locale);
  const { version } = useAppInfo();
  return (
    <Card title={t("about.title", locale)}>
      <div className="grid grid-cols-[100px_1fr] gap-y-2 text-sm">
        <span className="text-[#9ca3af]">{t("app.name", locale)}</span>
        <span className="text-[#e5e7eb]">OmniProxy</span>

        <span className="text-[#9ca3af]">{t("about.version", locale)}</span>
        <span className="text-[#e5e7eb] tabular-nums">
          v{version ?? "…"}
        </span>

        <span className="text-[#9ca3af]">{t("about.author", locale)}</span>
        <a
          className="text-[#3b82f6] hover:underline cursor-pointer"
          onClick={() => openExternalUrl("https://blog.022025.xyz")}
        >
          {t("about.authorValue", locale)}
        </a>

        <span className="text-[#9ca3af]">{t("about.blog", locale)}</span>
        <a
          className="text-[#3b82f6] hover:underline cursor-pointer tabular-nums"
          onClick={() => openExternalUrl("https://blog.022025.xyz")}
        >
          https://blog.022025.xyz
        </a>

        <span className="text-[#9ca3af]">{t("about.github", locale)}</span>
        <a
          className="text-[#3b82f6] hover:underline cursor-pointer tabular-nums"
          onClick={() => openExternalUrl("https://github.com/kurashizu/OmniProxy")}
        >
          https://github.com/kurashizu/OmniProxy
        </a>

        <span className="text-[#9ca3af]">{t("about.license", locale)}</span>
        <span className="text-[#e5e7eb]">{t("about.licenseValue", locale)}</span>
      </div>
    </Card>
  );
}
