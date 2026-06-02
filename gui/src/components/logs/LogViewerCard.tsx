"use client";
import { useEffect, useMemo, useRef, useState } from "react";
import { Card } from "@/components/common/Card";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";
import { formatTimestamp } from "@/lib/format";
import { Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import type { LogEntry } from "@/lib/schema";

export function LogViewerCard() {
  const locale = useAppStore((s) => s.locale);
  const paused = useAppStore((s) => s.logsPaused);
  const buffer = useAppStore((s) => s.logBuffer);
  const ref = useRef<VirtuosoHandle | null>(null);
  const [filter, setFilter] = useState({ search: "", level: "all", source: "all" });

  useEffect(() => {
    const id = window.setInterval(() => {
      const f = (window as any).__logFilter;
      if (f) setFilter(f);
    }, 200);
    return () => window.clearInterval(id);
  }, []);

  const filtered = useMemo(() => {
    const q = filter.search.trim().toLowerCase();
    return buffer.filter((e) => {
      if (filter.source !== "all" && e.stream !== "stdout" && e.stream !== "stderr") return false;
      if (filter.level !== "all" && !e.line.toUpperCase().includes(filter.level.toUpperCase())) return false;
      if (q && !e.line.toLowerCase().includes(q)) return false;
      return true;
    });
  }, [buffer, filter]);

  useEffect(() => {
    if (paused) return;
    ref.current?.scrollToIndex({ index: filtered.length - 1, behavior: "auto" });
  }, [filtered.length, paused]);

  return (
    <Card title={t("logs.title", locale)} className="h-full" bodyClassName="p-0 min-h-0">
      <div className="h-full font-mono text-[12px] leading-relaxed">
        {buffer.length === 0 ? (
          <div className="flex h-full items-center justify-center text-[#6b7280]">{t("logs.empty", locale)}</div>
        ) : (
          <Virtuoso
            ref={ref}
            data={filtered}
            followOutput={!paused}
            computeItemKey={(_, item) => `${item.ts_ms}-${item.line}`}
            itemContent={(_, entry) => <LogLine entry={entry} />}
            className="h-full bg-surface"
          />
        )}
      </div>
    </Card>
  );
}

function LogLine({ entry }: { entry: LogEntry }) {
  const level = entry.line.toUpperCase().includes("ERROR") ? "#ef4444" : entry.line.toUpperCase().includes("WARN") ? "#f59e0b" : "#3b82f6";
  const label = entry.line.toUpperCase().includes("ERROR") ? "ERROR" : entry.line.toUpperCase().includes("WARN") ? "WARN" : "INFO";
  return (
    <div className="flex items-start gap-2 px-3 py-0.5 hover:bg-card">
      <span className="text-[#6b7280] tabular-nums shrink-0">{formatTimestamp(entry.ts_ms)}</span>
      <span className="shrink-0 rounded px-1 text-[10px] font-medium" style={{ background: `${level}33`, color: level }}>{label}</span>
      <span className="text-text whitespace-pre-wrap break-all">{entry.line}</span>
    </div>
  );
}
