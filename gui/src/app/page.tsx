"use client";
import { useCallback, useEffect, useRef, useState } from "react";
import { NodeCard } from "@/components/home/NodeCard";
import { ConnectionStatusCard } from "@/components/home/ConnectionStatusCard";
import { TrafficCard } from "@/components/home/TrafficCard";
import { Dialog } from "@/components/common/Dialog";
import { useProxyState } from "@/hooks/useProxyState";
import { useProxyStats, useClientStats } from "@/hooks/useAdminPoll";
import { ipc } from "@/lib/ipc";
import type { GuiConfig } from "@/lib/schema";

export default function HomePage() {
  const { state } = useProxyState();
  const isRunning = state.state === "running";
  const { data: proxyStats } = useProxyStats(isRunning);
  const { data: clientStats } = useClientStats(isRunning);
  const [cfg, setCfg] = useState<GuiConfig | null>(null);
  const [dialogMsg, setDialogMsg] = useState<string | null>(null);
  const lastMsg = useRef<string | null>(null);

  useEffect(() => {
    if (state.state === "stopped" && state.message && state.message !== lastMsg.current) {
      lastMsg.current = state.message;
      setDialogMsg(state.message);
    }
  }, [state]);

  useEffect(() => { ipc.getGuiConfig().then(setCfg).catch(() => {}); }, []);

  return (
    <>
      <Dialog open={dialogMsg != null} title="Connection Error" message={dialogMsg ?? ""}
        onClose={useCallback(() => setDialogMsg(null), [])} />
      <div className="flex flex-col gap-4 pb-4">
        <div className="grid grid-cols-2 gap-4 min-w-0">
          <div className="flex-1 min-w-0">
            <NodeCard config={cfg} stats={clientStats} state={state.state} />
          </div>
          <div className="flex-1 min-w-0">
            <ConnectionStatusCard clientStats={clientStats} proxyStats={proxyStats} state={state.state} />
          </div>
        </div>
        <div className="min-w-0">
          <TrafficCard stats={clientStats} />
        </div>
      </div>
    </>
  );
}
