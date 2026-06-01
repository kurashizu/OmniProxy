"use client";
import { useProxyState } from "@/hooks/useProxyState";
import { useClientConnections } from "@/hooks/useAdminPoll";
import { ConnectionSummaryCard } from "@/components/connections/ConnectionSummaryCard";
import { ConnectionSearchCard } from "@/components/connections/ConnectionSearchCard";
import { ConnectionListCard } from "@/components/connections/ConnectionListCard";

export default function ConnectionsPage() {
  const { state } = useProxyState();
  const isRunning = state.state === "running";
  const { data } = useClientConnections(isRunning);
  const conns = data?.connections ?? [];

  return (
    <div className="grid grid-cols-1 gap-4">
      <ConnectionSummaryCard connections={conns} />
      <ConnectionSearchCard />
      <ConnectionListCard connections={conns} />
    </div>
  );
}
