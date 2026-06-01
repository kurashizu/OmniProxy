"use client";
import { useProxyState } from "@/hooks/useProxyState";
import { useProxyRoutes, useProxyStats } from "@/hooks/useAdminPoll";
import { TunInfoCard } from "@/components/routes/TunInfoCard";
import { RoutesTableCard } from "@/components/routes/RoutesTableCard";

export default function RoutesPage() {
  const { state } = useProxyState();
  const isRunning = state.state === "running";
  const { data: stats } = useProxyStats(isRunning);
  const { data: routes } = useProxyRoutes(isRunning);

  return (
    <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
      <TunInfoCard stats={stats} />
      <RoutesTableCard routes={routes ?? []} />
    </div>
  );
}
