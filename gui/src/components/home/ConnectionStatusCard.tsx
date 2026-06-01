"use client";
import { Card } from "@/components/common/Card";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";
import { formatBytes, formatDuration } from "@/lib/format";
import type { ClientStats, ProxyStats } from "@/lib/schema";

interface ConnectionStatusCardProps {
  clientStats: ClientStats | null | undefined;
  proxyStats: ProxyStats | null | undefined;
  state: string;
}

export function ConnectionStatusCard({
  clientStats,
  proxyStats,
  state,
}: ConnectionStatusCardProps) {
  const locale = useAppStore((s) => s.locale);
  const isRunning = state === "running";
  const proxyAlive = isRunning;
  const clientAlive = clientStats?.connected === true;
  const uptime = clientStats?.uptime_secs ?? 0;
  const latencyMs = clientStats?.latency_ms;

  return (
    <Card title={t("conn.title", locale)}>
      <div className="grid grid-cols-[100px_1fr] gap-y-2 text-sm">
        <span className="text-[#9ca3af]">{t("conn.proxyState", locale)}</span>
        <StatusPill
          ok={proxyAlive}
          label={proxyAlive ? t("node.connected", locale) : t("node.disconnected", locale)}
          okColor="#10b981"
        />

        <span className="text-[#9ca3af]">{t("conn.clientState", locale)}</span>
        <StatusPill
          ok={clientAlive}
          label={clientAlive ? t("node.connected", locale) : t("node.disconnected", locale)}
          okColor="#10b981"
        />

        <span className="text-[#9ca3af]">{t("conn.uptime", locale)}</span>
        <span className="text-[#e5e7eb] tabular-nums">
          {formatDuration(uptime)}
        </span>

        <span className="text-[#9ca3af]">{t("conn.reconnects", locale)}</span>
        <span className="text-[#e5e7eb] tabular-nums">
          {clientStats?.reconnect_count ?? 0}
        </span>

        <span className="text-[#9ca3af]">{t("conn.latency", locale)}</span>
        <span className="text-[#e5e7eb] tabular-nums">
          {latencyMs == null
            ? "—"
            : latencyMs > 1_000_000
              ? t("conn.timeout", locale)
              : `${latencyMs} ms`}
        </span>

        <span className="text-[#9ca3af]">{t("conn.jitter", locale)}</span>
        <span className="text-[#e5e7eb] tabular-nums">
          {clientStats?.latency_jitter_ms ?? 0} ms
        </span>

        <span className="text-[#9ca3af]">{t("conn.socks5", locale)}</span>
        <span className="text-[#e5e7eb] tabular-nums">
          127.0.0.1:{proxyStats?.socks_port ?? 0}
        </span>
      </div>
    </Card>
  );
}

function StatusPill({
  ok,
  label,
  okColor,
}: {
  ok: boolean;
  label: string;
  okColor: string;
}) {
  const color = ok ? okColor : "#6b7280";
  return (
    <span
      className="inline-flex w-fit items-center gap-1.5 rounded-full px-2 py-0.5 text-[11px] font-medium"
      style={{ background: `${color}33`, color }}
    >
      <span className="h-1.5 w-1.5 rounded-full" style={{ background: color }} />
      {label}
    </span>
  );
}
