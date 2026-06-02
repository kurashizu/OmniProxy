"use client";
import { Card } from "@/components/common/Card";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";
import { formatDuration } from "@/lib/format";
import type { ClientStats, ProxyStats } from "@/lib/schema";

export function ConnectionStatusCard({ clientStats, proxyStats, state }: {
  clientStats: ClientStats | null | undefined;
  proxyStats: ProxyStats | null | undefined;
  state: string;
}) {
  const locale = useAppStore((s) => s.locale);
  const isRunning = state === "running";
  const clientAlive = clientStats?.connected === true;
  const uptime = clientStats?.uptime_secs ?? 0;
  const latencyMs = clientStats?.latency_ms;

  return (
    <Card title={t("conn.title", locale)}>
      <div className="grid grid-cols-[96px_1fr] gap-y-1.5 text-[13px]">
        <StatusRow label={t("conn.proxyState", locale)} ok={isRunning} />
        <StatusRow label={t("conn.clientState", locale)} ok={clientAlive} />
        <DurationRow label={t("conn.uptime", locale)} value={formatDuration(uptime)} />
        <NumberRow label={t("conn.reconnects", locale)} value={clientStats?.reconnect_count ?? 0} />
        <span className="text-muted">{t("conn.latency", locale)}</span>
        <span className="text-text tabular-nums">
          {latencyMs == null ? "\u2014" : latencyMs > 1_000_000 ? t("conn.timeout", locale) : `${latencyMs} ms`}
        </span>
        <span className="text-muted">{t("conn.jitter", locale)}</span>
        <span className="text-text tabular-nums">{clientStats?.latency_jitter_ms ?? 0} ms</span>
        <span className="text-muted">{t("conn.socks5", locale)}</span>
        <span className="text-text tabular-nums">127.0.0.1:{proxyStats?.socks_port ?? 0}</span>
      </div>
    </Card>
  );
}

function StatusRow({ label, ok }: { label: string; ok: boolean }) {
  const color = ok ? "#10b981" : "#6b7280";
  return (
    <>
      <span className="text-muted">{label}</span>
      <span className="inline-flex w-fit items-center gap-1.5 rounded-full px-2 py-0.5 text-[11px] font-medium" style={{ background: `${color}33`, color }}>
        <span className="h-1.5 w-1.5 rounded-full" style={{ background: color }} />
        {ok ? "Connected" : "Disconnected"}
      </span>
    </>
  );
}

function DurationRow({ label, value }: { label: string; value: string }) {
  return <><span className="text-muted">{label}</span><span className="text-text tabular-nums">{value}</span></>;
}

function NumberRow({ label, value }: { label: string; value: number }) {
  return <><span className="text-muted">{label}</span><span className="text-text tabular-nums">{value}</span></>;
}
