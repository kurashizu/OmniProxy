"use client";
import { useEffect, useState } from "react";
import { NodeFormCard } from "@/components/settings/NodeFormCard";
import { LanguageSwitcher } from "@/components/common/LanguageSwitcher";
import { useProxyState } from "@/hooks/useProxyState";
import { useAppInfo } from "@/hooks/useAppInfo";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";
import { openExternalUrl } from "@/lib/ipc";
import { ipc } from "@/lib/ipc";
import type { GuiConfig } from "@/lib/schema";

export default function SettingsPage() {
  const { state } = useProxyState();
  const locale = useAppStore((s) => s.locale);
  const { version } = useAppInfo();
  const [cfg, setCfg] = useState<GuiConfig | null>(null);

  useEffect(() => { ipc.getGuiConfig().then(setCfg).catch(() => setCfg(null)); }, []);

  return (
    <div className="h-full overflow-y-auto">
      <NodeFormCard initial={cfg} state={state} onSaved={setCfg} />

      <div className="mt-4 rounded-lg border border-border bg-card p-4 text-sm text-muted">
        <div className="flex items-center justify-between">
          <span className="text-[11px] uppercase tracking-wider text-[#6b7280]">{t("settings.language", locale)}</span>
          <LanguageSwitcher />
        </div>
      </div>

      <div className="mt-4 rounded-lg border border-border bg-card p-4 text-sm text-muted">
        <div className="grid grid-cols-[100px_1fr] gap-y-1.5">
          <span>{t("app.name", locale)}</span>
          <span className="text-text">OmniProxy</span>
          <span>{t("about.version", locale)}</span>
          <span className="text-text tabular-nums">v{version ?? "\u2026"}</span>
          <span>{t("about.author", locale)}</span>
          <a className="text-primary hover:underline cursor-pointer" onClick={() => openExternalUrl("https://blog.022025.xyz")}>
            {t("about.authorValue", locale)}
          </a>
          <span>{t("about.github", locale)}</span>
          <a className="text-primary hover:underline cursor-pointer tabular-nums" onClick={() => openExternalUrl("https://github.com/kurashizu/OmniProxy")}>
            github.com/kurashizu/OmniProxy
          </a>
          <span>{t("about.license", locale)}</span>
          <span className="text-text">{t("about.licenseValue", locale)}</span>
        </div>
      </div>
    </div>
  );
}
