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

  return (
    <div className="grid h-full grid-rows-[auto_auto_1fr] gap-3 min-h-0">
      <ConnectionSummaryCard connections={data?.connections ?? []} />
      <ConnectionSearchCard />
      <div className="flex-1 min-h-0">
        <ConnectionListCard connections={data?.connections ?? []} />
      </div>
    </div>
  );
}
