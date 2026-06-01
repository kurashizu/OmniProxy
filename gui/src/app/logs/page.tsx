"use client";
import { LogFilterCard } from "@/components/logs/LogFilterCard";
import { LogViewerCard } from "@/components/logs/LogViewerCard";
import { useProxyLog } from "@/hooks/useProxyLog";

export default function LogsPage() {
  useProxyLog();
  return (
    <div className="grid grid-cols-1 gap-4">
      <LogFilterCard />
      <LogViewerCard />
    </div>
  );
}
