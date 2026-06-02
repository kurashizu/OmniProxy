"use client";
import { useMemo } from "react";
import { Card } from "@/components/common/Card";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";
import { formatDuration } from "@/lib/format";
import type { Connection } from "@/lib/schema";

const MAX = 200;

export function ConnectionListCard({ connections }: { connections: Connection[] }) {
  const locale = useAppStore((s) => s.locale);
  const filter = useAppStore((s) => s.connectionFilter);

  const visible = useMemo(() => {
    const q = filter.search.trim().toLowerCase();
    const f = connections.filter((c) => {
      if (!filter.protocol[c.protocol as "TCP" | "UDP" | "ICMP"]) return false;
      if (!q) return true;
      return c.target.toLowerCase().includes(q) || c.source.toLowerCase().includes(q) || String(c.id).includes(q);
    });
    f.sort((a, b) => {
      switch (filter.sort) {
        case "durationDesc": return b.duration_secs - a.duration_secs;
        case "durationAsc": return a.duration_secs - b.duration_secs;
        case "idDesc": return b.id - a.id;
        case "idAsc": return a.id - b.id;
      }
    });
    return f;
  }, [connections, filter]);

  const truncated = visible.length > MAX;
  const shown = truncated ? visible.slice(0, MAX) : visible;
  const empty = connections.length === 0;

  return (
    <Card
      title={t("connList.title", locale)}
      extra={<span className="text-xs text-muted">{visible.length}/{connections.length}</span>}
      className="h-full"
      bodyClassName="p-0 min-h-0"
    >
      <div className="h-full overflow-auto">
        {empty ? (
          <div className="px-4 py-8 text-center text-sm text-[#6b7280]">{t("connList.empty", locale)}</div>
        ) : shown.length === 0 ? (
          <div className="px-4 py-8 text-center text-sm text-[#6b7280]">{t("connList.empty.filtered", locale)}</div>
        ) : (
          <table className="w-full text-sm">
            <thead className="text-[11px] uppercase text-muted sticky top-0 bg-card z-10">
              <tr>
                <th className="px-2 py-1.5 text-left">{t("connList.id", locale)}</th>
                <th className="px-2 py-1.5 text-left">{t("connList.protocol", locale)}</th>
                <th className="px-2 py-1.5 text-left">{t("connList.target", locale)}</th>
                <th className="px-2 py-1.5 text-left">{t("connList.source", locale)}</th>
                <th className="px-2 py-1.5 text-right">{t("connList.duration", locale)}</th>
              </tr>
            </thead>
            <tbody>
              {shown.map((c) => (
                <tr key={c.id} className="border-t border-border hover:bg-[#1d212a]">
                  <td className="px-2 py-1.5 text-muted tabular-nums">{c.id}</td>
                  <td className="px-2 py-1.5">
                    <span className="rounded px-1.5 py-0.5 text-[10px] font-medium"
                      style={{ background: `${c.protocol === "TCP" ? "#3b82f6" : c.protocol === "UDP" ? "#10b981" : "#f59e0b"}33`,
                        color: c.protocol === "TCP" ? "#3b82f6" : c.protocol === "UDP" ? "#10b981" : "#f59e0b" }}>
                      {c.protocol}
                    </span>
                  </td>
                  <td className="px-2 py-1.5 text-text truncate max-w-[280px]" title={c.target}>{c.target || "\u2014"}</td>
                  <td className="px-2 py-1.5 text-muted truncate max-w-[200px]" title={c.source}>{c.source || "\u2014"}</td>
                  <td className="px-2 py-1.5 text-right text-text tabular-nums">{formatDuration(c.duration_secs)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
      {truncated && <div className="px-4 py-1.5 text-[11px] text-[#6b7280] border-t border-border">{t("connList.truncated", locale, { n: MAX })}</div>}
    </Card>
  );
}
