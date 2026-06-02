"use client";
import { LogFilterCard } from "@/components/logs/LogFilterCard";
import { LogViewerCard } from "@/components/logs/LogViewerCard";

export default function LogsPage() {
  return (
    <div className="grid h-full grid-rows-[auto_1fr] gap-3 min-h-0">
      <LogFilterCard />
      <div className="flex-1 min-h-0">
        <LogViewerCard />
      </div>
    </div>
  );
}
