"use client";
import { Card } from "@/components/common/Card";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";
import type { ProxyStats } from "@/lib/schema";

export function TunInfoCard({ stats }: { stats: ProxyStats | null | undefined }) {
  const locale = useAppStore((s) => s.locale);
  const tun = stats?.tun;
  return (
    <Card title={t("tun.title", locale)}>
      <div className="grid grid-cols-[120px_1fr] gap-y-1.5 text-sm">
        <span className="text-muted">{t("tun.iface", locale)}</span>
        <span className="text-text">{tun?.name || "\u2014"}</span>
        <span className="text-muted col-span-2 mt-2 text-xs uppercase tracking-wider">{t("tun.ipv4", locale)}</span>
        <span className="text-muted">{t("tun.address", locale)}</span>
        <span className="text-text tabular-nums">{tun?.ip || "\u2014"}</span>
        <span className="text-muted col-span-2 mt-2 text-xs uppercase tracking-wider">{t("tun.ipv6", locale)}</span>
        <span className="text-muted">{t("tun.address", locale)}</span>
        <span className="text-muted italic">(managed by proxy)</span>
      </div>
    </Card>
  );
}
