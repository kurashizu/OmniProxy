"use client";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { NodeCard } from "@/components/home/NodeCard";
import { ConnectionStatusCard } from "@/components/home/ConnectionStatusCard";
import { TrafficCard } from "@/components/home/TrafficCard";
import { useProxyState } from "@/hooks/useProxyState";
import { useProxyStats, useClientStats } from "@/hooks/useAdminPoll";
import { useBinaryPresent } from "@/hooks/useElevated";
import { ipc } from "@/lib/ipc";
import type { GuiConfig } from "@/lib/schema";

export default function HomePage() {
  const { state } = useProxyState();
  const isRunning = state.state === "running";
  const { data: proxyStats } = useProxyStats(isRunning);
  const { data: clientStats } = useClientStats(isRunning);
  const present = useBinaryPresent();
  const [cfg, setCfg] = useState<GuiConfig | null>(null);

  useEffect(() => {
    ipc
      .getGuiConfig()
      .then(setCfg)
      .catch(() => setCfg(null));
  }, []);

  return (
    <div className="grid grid-cols-1 lg:grid-cols-2 gap-4 h-full">
      <NodeCard
        config={cfg}
        stats={clientStats}
        state={state.state}
        binaryPresent={present}
      />
      <ConnectionStatusCard
        clientStats={clientStats}
        proxyStats={proxyStats}
        state={state.state}
      />
      <div className="lg:col-span-2 min-h-0">
        <TrafficCard stats={clientStats} />
      </div>
    </div>
  );
}
