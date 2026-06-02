"use client";
import { useMemo } from "react";
import type { AlignedData } from "uplot";
import { Card } from "@/components/common/Card";
import { Chart } from "@/components/common/Chart";
import { StatTile } from "@/components/common/StatTile";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";
import { formatBytes } from "@/lib/format";
import { useTrafficSamples } from "@/hooks/useTrafficSamples";
import type { ClientStats } from "@/lib/schema";

export function TrafficCard({ stats }: { stats: ClientStats | null | undefined }) {
  const locale = useAppStore((s) => s.locale);
  const { samples, latest } = useTrafficSamples(stats);
  const txTotal = stats?.bytes.tx ?? 0;
  const rxTotal = stats?.bytes.rx ?? 0;

  const data = useMemo<AlignedData>(() => {
    if (samples.length === 0) return [[0], [0], [0]];
    const xs = new Array(samples.length);
    const up = new Array(samples.length);
    const down = new Array(samples.length);
    for (let i = 0; i < samples.length; i++) {
      xs[i] = samples[i].t;
      up[i] = samples[i].up;
      down[i] = samples[i].down;
    }
    return [xs, up, down];
  }, [samples]);

  return (
    <Card title={t("traffic.title", locale)}>
      <div className="flex flex-col gap-3">
        <div className="rounded-md border border-border bg-surface p-2">
          <div className="h-56">
            <Chart
              data={data}
              height={220}
              legendShow={false}
              series={[
                {},
                { label: "Up", stroke: "#3b82f6", width: 1.5, fill: "rgba(59,130,246,0.15)" },
                { label: "Down", stroke: "#a855f7", width: 1.5, fill: "rgba(168,85,247,0.15)" },
              ]}
              axes={[
                { stroke: "#e5e7eb", font: "11px system-ui, sans-serif", grid: { stroke: "rgba(229,231,235,0.08)", width: 1 }, ticks: { stroke: "rgba(229,231,235,0.18)", width: 1 } },
                { stroke: "#e5e7eb", font: "11px system-ui, sans-serif", grid: { stroke: "rgba(229,231,235,0.08)", width: 1 }, ticks: { stroke: "rgba(229,231,235,0.18)", width: 1 } },
              ]}
            />
          </div>
        </div>

        <div className="grid grid-cols-2 xl:grid-cols-4 gap-2">
          <StatTile label={`↑ ${t("traffic.up", locale)}`} value={formatBytes(latest?.up ?? 0, { perSec: true })} />
          <StatTile label={`↓ ${t("traffic.down", locale)}`} value={formatBytes(latest?.down ?? 0, { perSec: true })} />
          <StatTile label={`↑ ${t("traffic.totalUp", locale)}`} value={formatBytes(txTotal)} />
          <StatTile label={`↓ ${t("traffic.totalDown", locale)}`} value={formatBytes(rxTotal)} />
        </div>
      </div>
    </Card>
  );
}
