"use client";
import { useMemo } from "react";
import { Card } from "@/components/common/Card";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";
import type { Connection } from "@/lib/schema";

export function ConnectionSummaryCard({
  connections,
}: {
  connections: Connection[];
}) {
  const locale = useAppStore((s) => s.locale);
  const counts = useMemo(() => {
    let tcp = 0;
    let udp = 0;
    let icmp = 0;
    for (const c of connections) {
      if (c.protocol === "TCP") tcp++;
      else if (c.protocol === "UDP") udp++;
      else if (c.protocol === "ICMP") icmp++;
    }
    return { tcp, udp, icmp, total: tcp + udp + icmp };
  }, [connections]);

  return (
    <Card title={t("connList.title", locale)}>
      <div className="grid grid-cols-4 gap-2 text-center">
        <CountCell label="TCP" value={counts.tcp} color="#3b82f6" />
        <CountCell label="UDP" value={counts.udp} color="#10b981" />
        <CountCell label="ICMP" value={counts.icmp} color="#f59e0b" />
        <CountCell label="Σ" value={counts.total} color="#9ca3af" />
      </div>
    </Card>
  );
}

function CountCell({
  label,
  value,
  color,
}: {
  label: string;
  value: number;
  color: string;
}) {
  return (
    <div className="rounded-md border border-[#252934] bg-[#0f1115] p-2">
      <div className="text-[11px] text-[#9ca3af]">{label}</div>
      <div
        className="text-2xl font-semibold tabular-nums"
        style={{ color }}
      >
        {value}
      </div>
    </div>
  );
}
