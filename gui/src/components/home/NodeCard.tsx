"use client";
import { useState } from "react";
import { Card } from "@/components/common/Card";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";
import { copyToClipboard } from "@/lib/format";
import type { ClientStats, GuiConfig } from "@/lib/schema";

export function NodeCard({ config, stats, state }: {
  config: GuiConfig | null | undefined;
  stats: ClientStats | null | undefined;
  state: string;
}) {
  const locale = useAppStore((s) => s.locale);
  const node = config?.active_node != null ? config.nodes[config.active_node] : null;
  const si = stats?.server_info;
  const latencyMs = stats?.latency_ms;
  const latencyColor = !latencyMs ? "#6b7280" : latencyMs < 100 ? "#10b981" : latencyMs < 300 ? "#f59e0b" : "#ef4444";
  const isRunning = state === "running";
  const statusText = isRunning ? t("node.connected", locale) : t("node.disconnected", locale);
  const statusColor = isRunning ? "#10b981" : "#9ca3af";

  return (
    <Card title={t("node.title", locale)} className="min-h-0">
      <div className="flex flex-col gap-3">
        <div className="flex items-center gap-2 text-[13px]">
          <span className="text-primary shrink-0">
            <svg viewBox="0 0 20 20" fill="currentColor" width="18" height="18">
              <path d="M10.707 2.293a1 1 0 00-1.414 0l-7 7a1 1 0 001.414 1.414L4 10.414V17a1 1 0 001 1h2a1 1 0 001-1v-2a1 1 0 011-1h2a1 1 0 011 1v2a1 1 0 001 1h2a1 1 0 001-1v-6.586l.293.293a1 1 0 001.414-1.414l-7-7z" />
            </svg>
          </span>
          <span className="text-text font-medium">{node?.name ?? "\u2014"}</span>
          <span className="rounded-full px-2 py-0.5 text-[11px] font-medium" style={{ background: `${statusColor}33`, color: statusColor }}>
            {"\u25CF"} {statusText}
          </span>
          {latencyMs != null && (
            <span className="ml-auto rounded-full px-2 py-0.5 text-[11px] font-medium tabular-nums" style={{ background: `${latencyColor}33`, color: latencyColor }}>
              {latencyMs} ms
            </span>
          )}
        </div>
        <div className="grid grid-cols-[132px_minmax(0,1fr)] gap-y-1.25 text-[12px]">
          <LabelField label={t("node.server", locale)} value={node?.server} />
          <LabelField label={t("node.serverIp", locale)} value={si?.server_ip} />
          <LabelField label={t("node.clientOutboundV4", locale)} value={si?.client_outbound_ipv4} />
          <LabelField label={t("node.clientOutboundV6", locale)} value={si?.client_outbound_ipv6} />
          <LabelField label={t("node.serverOutboundV4", locale)} value={si?.server_outbound_ipv4} />
          <LabelField label={t("node.serverOutboundV6", locale)} value={si?.server_outbound_ipv6} />
        </div>
      </div>
    </Card>
  );
}

function LabelField({ label, value }: { label: string; value?: string | null }) {
  const locale = useAppStore((s) => s.locale);
  const [copied, setCopied] = useState(false);
  return (
    <>
      <span className="text-muted whitespace-nowrap">{label}</span>
      {value ? (
        <div className="flex items-center gap-2">
          <span className="text-text tabular-nums truncate" title={value}>{value}</span>
          <button
            onClick={async () => { await copyToClipboard(value); setCopied(true); setTimeout(() => setCopied(false), 1200); }}
            className="rounded border border-border bg-surface px-1.5 py-0.5 text-[10px] text-muted hover:text-text hover:border-primary"
          >
            {copied ? t("node.copied", locale) : t("node.copy", locale)}
          </button>
        </div>
      ) : (
        <span className="text-[#6b7280]">{"\u2014"}</span>
      )}
    </>
  );
}
