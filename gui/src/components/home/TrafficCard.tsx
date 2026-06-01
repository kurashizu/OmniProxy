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
  const active = stats
    ? stats.active.tcp + stats.active.udp + stats.active.icmp
    : 0;
  void active;

  const data = useMemo<AlignedData>(() => {
    if (samples.length === 0) {
      return [[0], [0], [0]];
    }
    return buildSeries(samples);
  }, [samples]);

  return (
    <Card title={t("traffic.title", locale)} className="h-full">
      <div className="flex flex-col gap-3 h-full">
        <div className="h-40 flex-none">
          <Chart
            data={data}
            height={160}
            series={[
              {},
              {
                label: `${t("traffic.up", locale)} (bytes/s)`,
                stroke: "#3b82f6",
                width: 1.5,
                fill: "rgba(59, 130, 246, 0.15)",
              },
              {
                label: `${t("traffic.down", locale)} (bytes/s)`,
                stroke: "#a855f7",
                width: 1.5,
                fill: "rgba(168, 85, 247, 0.15)",
              },
            ]}
          />
        </div>
        <div className="grid grid-cols-4 gap-2 flex-none">
          <StatTile
            label={`↑ ${t("traffic.up", locale)}`}
            value={formatBytes(latest?.up ?? 0, { perSec: true })}
            color="#3b82f6"
          />
          <StatTile
            label={`↓ ${t("traffic.down", locale)}`}
            value={formatBytes(latest?.down ?? 0, { perSec: true })}
            color="#a855f7"
          />
          <StatTile
            label={`↑ ${t("traffic.totalUp", locale)}`}
            value={formatBytes(txTotal)}
            color="#3b82f6"
          />
          <StatTile
            label={`↓ ${t("traffic.totalDown", locale)}`}
            value={formatBytes(rxTotal)}
            color="#a855f7"
          />
        </div>
      </div>
    </Card>
  );
}

function buildSeries(
  samples: { t: number; up: number; down: number }[],
): AlignedData {
  const xs: number[] = new Array(samples.length);
  const up: number[] = new Array(samples.length);
  const down: number[] = new Array(samples.length);
  for (let i = 0; i < samples.length; i++) {
    xs[i] = samples[i].t;
    up[i] = samples[i].up;
    down[i] = samples[i].down;
  }
  return [xs, up, down];
}
