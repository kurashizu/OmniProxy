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
          <span className="text-muted shrink-0">
            <svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14">
              <path fillRule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clipRule="evenodd" />
            </svg>
          </span>
          <input value={search} onChange={(e) => setSearch(e.target.value)} placeholder={t("logs.filter.search", locale)} className="flex-1" />
        </div>
        <div className="flex flex-wrap items-center gap-3 text-xs">
          <span className="text-muted">{t("logs.filter.level", locale)}:</span>
          <select value={level} onChange={(e) => setLevel(e.target.value as typeof level)}>
            <option value="all">{t("filter.level.all", locale)}</option>
            <option value="info">{t("filter.level.info", locale)}</option>
            <option value="warn">{t("filter.level.warn", locale)}</option>
            <option value="error">{t("filter.level.error", locale)}</option>
          </select>
          <span className="text-muted">{t("logs.filter.source", locale)}:</span>
          <select value={source} onChange={(e) => setSource(e.target.value as typeof source)}>
            <option value="all">{t("logs.filter.all", locale)}</option>
            <option value="proxy">{t("logs.filter.proxy", locale)}</option>
            <option value="client">{t("logs.filter.client", locale)}</option>
          </select>
          <div className="ml-auto flex items-center gap-2">
            <button onClick={() => setPaused(!paused)}
              className="rounded border border-border bg-surface px-2 py-0.5 text-muted hover:text-text hover:border-primary"
            >
              {paused ? t("logs.resume", locale) : t("logs.pause", locale)}
            </button>
            <button onClick={clearLogs}
              className="rounded border border-border bg-surface px-2 py-0.5 text-muted hover:text-text hover:border-primary"
            >
              {t("logs.clear", locale)}
            </button>
          </div>
        </div>
        <div className="text-[11px] text-[#6b7280]">
          {t("logs.buffer", locale, { n: logCount })}
          {dropped > 0 && ` \u00B7 ${t("logs.dropped", locale, { n: dropped })}`}
        </div>
        <LogFilterSync search={search} level={level} source={source} />
      </div>
    </Card>
  );
}

function LogFilterSync(props: { search: string; level: string; source: string }) {
  if (typeof window !== "undefined") (window as any).__logFilter = props;
  return null;
}
