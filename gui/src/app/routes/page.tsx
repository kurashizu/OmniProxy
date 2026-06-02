"use client";
import { useProxyState } from "@/hooks/useProxyState";
import { useProxyStats, useProxyRoutes } from "@/hooks/useAdminPoll";
import { TunInfoCard } from "@/components/routes/TunInfoCard";
import { RoutesTableCard } from "@/components/routes/RoutesTableCard";

export default function RoutesPage() {
  const { state } = useProxyState();
  const isRunning = state.state === "running";
  const { data: stats } = useProxyStats(isRunning);
  const { data: routes } = useProxyRoutes(isRunning);

  return (
    <div className="grid h-full grid-rows-[auto_1fr] gap-3 min-h-0">
      <TunInfoCard stats={stats} />
      <div className="flex-1 min-h-0">
        <RoutesTableCard routes={routes ?? []} />
      </div>
    </div>
  );
}
