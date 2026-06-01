"use client";
import { useEffect, useState } from "react";
import { NodeFormCard } from "@/components/settings/NodeFormCard";
import { AboutCard } from "@/components/settings/AboutCard";
import { useProxyState } from "@/hooks/useProxyState";
import { ipc } from "@/lib/ipc";
import type { GuiConfig } from "@/lib/schema";

export default function SettingsPage() {
  const { state } = useProxyState();
  const [cfg, setCfg] = useState<GuiConfig | null>(null);

  useEffect(() => {
    ipc
      .getGuiConfig()
      .then(setCfg)
      .catch(() => setCfg(null));
  }, []);

  return (
    <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
      <NodeFormCard initial={cfg} state={state} onSaved={setCfg} />
      <AboutCard />
    </div>
  );
}
