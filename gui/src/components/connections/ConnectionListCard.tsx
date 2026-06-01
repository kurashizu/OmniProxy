"use client";
import { useMemo, useState, useEffect } from "react";
import { Card } from "@/components/common/Card";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";
import { formatDuration } from "@/lib/format";
import type { Connection } from "@/lib/schema";

const MAX_ROWS = 200;

export function ConnectionListCard({
  connections,
}: {
  connections: Connection[];
}) {
  const locale = useAppStore((s) => s.locale);
  const filter = useAppStore((s) => s.connectionFilter);
  const filtered = useMemo(() => {
    const q = filter.search.trim().toLowerCase();
    return connections.filter((c) => {
      if (!filter.protocol[c.protocol as "TCP" | "UDP" | "ICMP"]) return false;
      if (!q) return true;
      return (
        c.target.toLowerCase().includes(q) ||
        c.source.toLowerCase().includes(q) ||
        String(c.id).includes(q) ||
        c.protocol.toLowerCase().includes(q)
      );
    });
  }, [connections, filter]);

  const sorted = useMemo(() => {
    const arr = [...filtered];
    arr.sort((a, b) => {
      switch (filter.sort) {
        case "durationDesc":
          return b.duration_secs - a.duration_secs;
        case "durationAsc":
          return a.duration_secs - b.duration_secs;
        case "idDesc":
          return b.id - a.id;
        case "idAsc":
          return a.id - b.id;
      }
    });
    return arr;
  }, [filtered, filter.sort]);

  const truncated = sorted.length > MAX_ROWS;
  const visible = truncated ? sorted.slice(0, MAX_ROWS) : sorted;

  const isEmpty = connections.length === 0;
  const isFilteredEmpty = !isEmpty && filtered.length === 0;

  return (
    <Card
      title={t("connList.title", locale)}
      extra={
        <span className="text-xs text-[#9ca3af]">
          {filtered.length}/{connections.length}
        </span>
      }
      bodyClassName="p-0"
    >
      <div className="overflow-auto max-h-[60vh]">
        {isEmpty ? (
          <div className="px-4 py-8 text-center text-sm text-[#6b7280]">
            {t("connList.empty", locale)}
          </div>
        ) : isFilteredEmpty ? (
          <div className="px-4 py-8 text-center text-sm text-[#6b7280]">
            {t("connList.empty.filtered", locale)}
          </div>
        ) : (
          <table className="w-full text-sm">
            <thead className="text-[11px] uppercase text-[#9ca3af] sticky top-0 bg-[#171a21] z-10">
              <tr>
                <th className="px-2 py-1.5 text-left">{t("connList.id", locale)}</th>
                <th className="px-2 py-1.5 text-left">{t("connList.protocol", locale)}</th>
                <th className="px-2 py-1.5 text-left">{t("connList.target", locale)}</th>
                <th className="px-2 py-1.5 text-left">{t("connList.source", locale)}</th>
                <th className="px-2 py-1.5 text-right">{t("connList.duration", locale)}</th>
                <th className="px-2 py-1.5 text-right">{t("connList.action", locale)}</th>
              </tr>
            </thead>
            <tbody>
              {visible.map((c) => (
                <Row key={c.id} conn={c} />
              ))}
            </tbody>
          </table>
        )}
      </div>
      {truncated && (
        <div className="px-4 py-1.5 text-[11px] text-[#6b7280] border-t border-[#252934]">
          {t("connList.truncated", locale, { n: MAX_ROWS })}
        </div>
      )}
    </Card>
  );
}

function Row({ conn }: { conn: Connection }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);
  // approximate live duration: client_elapsed + (now - lastUpdate)
  // we don't know when the snapshot was made, so we just show the snapshot's value.
  // uPlot will catch real-time data; for the table we just use the latest.
  void now;
  const color =
    conn.protocol === "TCP"
      ? "#3b82f6"
      : conn.protocol === "UDP"
        ? "#10b981"
        : "#f59e0b";

  return (
    <tr className="border-t border-[#252934] hover:bg-[#1d212a]">
      <td className="px-2 py-1.5 text-[#9ca3af] tabular-nums">{conn.id}</td>
      <td className="px-2 py-1.5">
        <span
          className="rounded px-1.5 py-0.5 text-[10px] font-medium"
          style={{ background: `${color}33`, color }}
        >
          {conn.protocol}
        </span>
      </td>
      <td className="px-2 py-1.5 text-[#e5e7eb] truncate max-w-[280px]" title={conn.target}>
        {conn.target || "—"}
      </td>
      <td className="px-2 py-1.5 text-[#9ca3af] truncate max-w-[200px]" title={conn.source}>
        {conn.source || "—"}
      </td>
      <td className="px-2 py-1.5 text-right text-[#e5e7eb] tabular-nums">
        {formatDuration(conn.duration_secs)}
      </td>
      <td className="px-2 py-1.5 text-right">
        <button
          className="rounded border border-[#252934] bg-[#0f1115] px-2 py-0.5 text-[11px] text-[#9ca3af] hover:text-[#ef4444] hover:border-[#ef4444]"
          title="Close"
        >
          ×
        </button>
      </td>
    </tr>
  );
}
