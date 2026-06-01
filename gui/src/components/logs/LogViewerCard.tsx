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
  const [filter, setFilter] = useState<{ search: string; level: string; source: string }>({
    search: "",
    level: "all",
    source: "all",
  });

  useEffect(() => {
    const id = window.setInterval(() => {
      const f = (window as unknown as { __logFilter?: { search: string; level: string; source: string } }).__logFilter;
      if (f) setFilter(f);
    }, 200);
    return () => window.clearInterval(id);
  }, []);

  const filtered = useMemo(() => {
    const q = filter.search.trim().toLowerCase();
    return buffer.filter((e) => {
      if (filter.source !== "all") {
        if (filter.source === "proxy" && e.stream !== "stdout" && e.stream !== "stderr") return false;
        // 'client' is approximated as anything not from proxy stderr
        // (client logs come from a separate stream we currently pipe as
        // stdout from the same proxy process; a real distinction requires
        // the proxy to tag stream lines).
      }
      if (filter.level !== "all") {
        const l = filter.level.toUpperCase();
        if (!e.line.toUpperCase().includes(l)) return false;
      }
      if (q && !e.line.toLowerCase().includes(q)) return false;
      return true;
    });
  }, [buffer, filter]);

  // Auto-scroll to bottom when new entries arrive and not paused.
  useEffect(() => {
    if (paused) return;
    ref.current?.scrollToIndex({ index: filtered.length - 1, behavior: "auto" });
  }, [filtered.length, paused]);

  const empty = buffer.length === 0;

  return (
    <Card
      title={t("logs.title", locale)}
      bodyClassName="p-0"
    >
      <div className="h-[60vh] font-mono text-[12px] leading-relaxed">
        {empty ? (
          <div className="flex h-full items-center justify-center text-[#6b7280]">
            {t("logs.empty", locale)}
          </div>
        ) : (
          <Virtuoso
            ref={ref}
            data={filtered}
            followOutput={!paused}
            computeItemKey={(_, item) => `${item.ts_ms}-${item.line}`}
            itemContent={(_, entry) => <LogLine entry={entry} />}
            className="bg-[#0f1115]"
          />
        )}
      </div>
    </Card>
  );
}

function LogLine({ entry }: { entry: LogEntry }) {
  const levelColor = detectLevel(entry.line);
  return (
    <div className="flex items-start gap-2 px-3 py-0.5 hover:bg-[#171a21]">
      <span className="text-[#6b7280] tabular-nums shrink-0">
        {formatTimestamp(entry.ts_ms)}
      </span>
      <span
        className="shrink-0 rounded px-1 text-[10px] font-medium"
        style={{ background: `${levelColor}33`, color: levelColor }}
      >
        {detectLevelLabel(entry.line)}
      </span>
      <span className="text-[#e5e7eb] whitespace-pre-wrap break-all">
        {entry.line}
      </span>
    </div>
  );
}

function detectLevel(line: string): string {
  const u = line.toUpperCase();
  if (u.includes("ERROR")) return "#ef4444";
  if (u.includes("WARN")) return "#f59e0b";
  return "#3b82f6";
}
function detectLevelLabel(line: string): string {
  const u = line.toUpperCase();
  if (u.includes("ERROR")) return "ERROR";
  if (u.includes("WARN")) return "WARN";
  return "INFO";
}
