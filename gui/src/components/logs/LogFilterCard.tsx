"use client";
import { useState } from "react";
import { Card } from "@/components/common/Card";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";

export function LogFilterCard() {
  const locale = useAppStore((s) => s.locale);
  const paused = useAppStore((s) => s.logsPaused);
  const setPaused = useAppStore((s) => s.setLogsPaused);
  const clearLogs = useAppStore((s) => s.clearLogs);
  const dropped = useAppStore((s) => s.dropped);
  const logCount = useAppStore((s) => s.logBuffer.length);

  const [search, setSearch] = useState("");
  const [level, setLevel] = useState<"all" | "info" | "warn" | "error">("all");
  const [source, setSource] = useState<"all" | "proxy" | "client">("all");

  return (
    <Card title={t("logs.title", locale)}>
      <div className="flex flex-col gap-2">
        <div className="flex items-center gap-2">
          <span className="text-xs text-[#9ca3af]">🔍</span>
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t("logs.filter.search", locale)}
            className="flex-1"
          />
        </div>
        <div className="flex flex-wrap items-center gap-3 text-xs">
          <span className="text-[#9ca3af]">{t("logs.filter.level", locale)}:</span>
          <select
            value={level}
            onChange={(e) => setLevel(e.target.value as typeof level)}
          >
            <option value="all">{t("filter.level.all", locale)}</option>
            <option value="info">{t("filter.level.info", locale)}</option>
            <option value="warn">{t("filter.level.warn", locale)}</option>
            <option value="error">{t("filter.level.error", locale)}</option>
          </select>
          <span className="text-[#9ca3af]">{t("logs.filter.source", locale)}:</span>
          <select
            value={source}
            onChange={(e) => setSource(e.target.value as typeof source)}
          >
            <option value="all">{t("logs.filter.all", locale)}</option>
            <option value="proxy">{t("logs.filter.proxy", locale)}</option>
            <option value="client">{t("logs.filter.client", locale)}</option>
          </select>
          <div className="ml-auto flex items-center gap-2">
            <button
              onClick={() => setPaused(!paused)}
              className="rounded border border-[#252934] bg-[#0f1115] px-2 py-0.5 text-[#9ca3af] hover:text-[#e5e7eb] hover:border-[#3b82f6]"
            >
              {paused ? t("logs.resume", locale) : t("logs.pause", locale)}
            </button>
            <button
              onClick={clearLogs}
              className="rounded border border-[#252934] bg-[#0f1115] px-2 py-0.5 text-[#9ca3af] hover:text-[#e5e7eb] hover:border-[#3b82f6]"
            >
              {t("logs.clear", locale)}
            </button>
          </div>
        </div>
        <div className="text-[11px] text-[#6b7280]">
          {t("logs.buffer", locale, { n: logCount })}
          {dropped > 0 && ` · ${t("logs.dropped", locale, { n: dropped })}`}
        </div>
        {/* Provide the filter values to the LogViewerCard via global window — naive but works. */}
        <LogFilterSync search={search} level={level} source={source} />
      </div>
    </Card>
  );
}

// Push the current filter values to a global so the LogViewerCard can read
// them. Using a tiny store would be overkill for two siblings that are
// always mounted together on the same page.
function LogFilterSync({
  search,
  level,
  source,
}: {
  search: string;
  level: string;
  source: string;
}) {
  if (typeof window !== "undefined") {
    (window as unknown as { __logFilter?: unknown }).__logFilter = {
      search,
      level,
      source,
    };
  }
  return null;
}
