"use client";
import { useState } from "react";
import { Card } from "@/components/common/Card";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";
import { copyToClipboard } from "@/lib/format";
import type { ClientStats, GuiConfig } from "@/lib/schema";

interface NodeCardProps {
  config: GuiConfig | null | undefined;
  stats: ClientStats | null | undefined;
  state: string;
  binaryPresent: boolean | null;
}

export function NodeCard({ config, stats, state, binaryPresent }: NodeCardProps) {
  const locale = useAppStore((s) => s.locale);
  const node = config?.active_node != null ? config.nodes[config.active_node] : null;
  const serverInfo = stats?.server_info;
  const latencyMs = stats?.latency_ms;
  const latencyColor =
    latencyMs == null
      ? "#6b7280"
      : latencyMs < 100
        ? "#10b981"
        : latencyMs < 300
          ? "#f59e0b"
          : "#ef4444";

  const isRunning = state === "running";
  const statusText = isRunning
    ? t("node.connected", locale)
    : state === "starting" || state === "stopping"
      ? t("node.connecting", locale)
      : t("node.disconnected", locale);
  const statusColor = isRunning
    ? "#10b981"
    : state === "error"
      ? "#ef4444"
      : "#9ca3af";

  return (
    <Card
      title={t("node.title", locale)}
      className="min-h-0"
    >
      <div className="flex flex-col gap-3">
        <div className="flex items-center gap-2">
          <span className="text-base">🐾</span>
          <span className="text-[#e5e7eb] font-medium">
            {node?.name ?? "—"}
          </span>
          <span
            className="rounded-full px-2 py-0.5 text-[11px] font-medium"
            style={{ background: `${statusColor}33`, color: statusColor }}
          >
            ● {statusText}
          </span>
          {latencyMs != null && (
            <span
              className="ml-auto rounded-full px-2 py-0.5 text-[11px] font-medium tabular-nums"
              style={{ background: `${latencyColor}33`, color: latencyColor }}
            >
              {latencyMs} ms
            </span>
          )}
        </div>

        {binaryPresent === false && (
          <div className="rounded border border-[#ef4444]/30 bg-[#ef4444]/10 px-3 py-2 text-xs text-[#ef4444]">
            {t("power.binaryNotFound", locale)}
          </div>
        )}

        <div className="grid grid-cols-[120px_1fr] gap-y-1.5 text-sm">
          <span className="text-[#9ca3af]">{t("node.server", locale)}</span>
          <CopyableRow
            value={node?.server ?? ""}
            placeholder="—"
          />

          <span className="text-[#9ca3af]">{t("node.serverIp", locale)}</span>
          <CopyableRow
            value={serverInfo?.server_ip ?? null}
            placeholder="—"
          />

          <span className="text-[#9ca3af]">
            {t("node.clientOutboundV4", locale)}
          </span>
          <CopyableRow
            value={serverInfo?.client_outbound_ipv4 ?? null}
            placeholder="—"
          />

          <span className="text-[#9ca3af]">
            {t("node.clientOutboundV6", locale)}
          </span>
          <CopyableRow
            value={serverInfo?.client_outbound_ipv6 ?? null}
            placeholder="—"
          />

          <span className="text-[#9ca3af]">
            {t("node.serverOutboundV4", locale)}
          </span>
          <CopyableRow
            value={serverInfo?.server_outbound_ipv4 ?? null}
            placeholder="—"
          />

          <span className="text-[#9ca3af]">
            {t("node.serverOutboundV6", locale)}
          </span>
          <CopyableRow
            value={serverInfo?.server_outbound_ipv6 ?? null}
            placeholder="—"
          />
        </div>
      </div>
    </Card>
  );
}

function CopyableRow({
  value,
  placeholder,
}: {
  value: string | null | undefined;
  placeholder?: string;
}) {
  const locale = useAppStore((s) => s.locale);
  const [copied, setCopied] = useState(false);
  if (!value) {
    return <span className="text-[#6b7280]">{placeholder ?? "—"}</span>;
  }
  return (
    <div className="flex items-center gap-2">
      <span className="text-[#e5e7eb] tabular-nums truncate" title={value}>
        {value}
      </span>
      <button
        onClick={async () => {
          await copyToClipboard(value);
          setCopied(true);
          setTimeout(() => setCopied(false), 1200);
        }}
        className="rounded border border-[#252934] bg-[#0f1115] px-1.5 py-0.5 text-[10px] text-[#9ca3af] hover:text-[#e5e7eb] hover:border-[#3b82f6]"
      >
        {copied ? t("node.copied", locale) : t("node.copy", locale)}
      </button>
    </div>
  );
}
