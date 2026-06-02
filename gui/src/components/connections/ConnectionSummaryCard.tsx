"use client";
import { useMemo } from "react";
import { Card } from "@/components/common/Card";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";
import type { Connection } from "@/lib/schema";

export function ConnectionSummaryCard({ connections }: { connections: Connection[] }) {
  const locale = useAppStore((s) => s.locale);
  const counts = useMemo(() => {
    const c = { tcp: 0, udp: 0, icmp: 0 };
    for (const conn of connections) {
      if (conn.protocol === "TCP") c.tcp++;
      else if (conn.protocol === "UDP") c.udp++;
      else if (conn.protocol === "ICMP") c.icmp++;
    }
    return { ...c, total: c.tcp + c.udp + c.icmp };
  }, [connections]);

  return (
    <Card title={t("connList.title", locale)}>
      <div className="grid grid-cols-4 gap-2 text-center">
        <CountCell label="TCP" value={counts.tcp} color="#3b82f6" />
        <CountCell label="UDP" value={counts.udp} color="#10b981" />
        <CountCell label="ICMP" value={counts.icmp} color="#f59e0b" />
        <CountCell label={"\u03A3"} value={counts.total} color="#9ca3af" />
      </div>
    </Card>
  );
}

function CountCell({ label, value, color }: { label: string; value: number; color: string }) {
  return (
    <div className="rounded-md border border-border bg-surface p-2">
      <div className="text-[11px] text-muted">{label}</div>
      <div className="text-xl font-semibold tabular-nums leading-none" style={{ color }}>{value}</div>
    </div>
  );
}
