"use client";
import { Card } from "@/components/common/Card";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";

export function ConnectionSearchCard() {
  const locale = useAppStore((s) => s.locale);
  const filter = useAppStore((s) => s.connectionFilter);
  const setFilter = useAppStore((s) => s.setConnectionFilter);
  const paused = useAppStore((s) => s.connsPaused);
  const setPaused = useAppStore((s) => s.setConnsPaused);

  return (
    <Card
      title={t("connList.title", locale)}
      extra={
        <button onClick={() => setPaused(!paused)}
          className="rounded border border-border bg-surface px-2 py-0.5 text-muted hover:text-text hover:border-primary text-xs"
        >
          {paused ? t("connList.resume", locale) : t("connList.pause", locale)}
        </button>
      }
    >
      <div className="flex flex-col gap-2">
        <div className="flex items-center gap-2">
          <span className="text-muted shrink-0">
            <svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14">
              <path fillRule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clipRule="evenodd" />
            </svg>
          </span>
          <input value={filter.search} onChange={(e) => setFilter({ search: e.target.value })}
            placeholder={t("connList.search", locale)} className="flex-1" />
        </div>
        <div className="flex flex-wrap items-center gap-3 text-xs">
          <span className="text-muted">{t("connList.filter.protocol", locale)}:</span>
          {(["TCP", "UDP", "ICMP"] as const).map((p) => (
            <label key={p} className="flex items-center gap-1 cursor-pointer">
              <input type="checkbox" checked={filter.protocol[p]}
                onChange={() => setFilter({ protocol: { ...filter.protocol, [p]: !filter.protocol[p] } })} />
              <span>{p}</span>
            </label>
          ))}
          <span className="ml-auto flex items-center gap-2">
            <span className="text-muted">{t("connList.sort", locale)}:</span>
            <select value={filter.sort} onChange={(e) => setFilter({ sort: e.target.value as typeof filter.sort })} className="text-xs">
              <option value="durationDesc">{t("connList.sort.durationDesc", locale)}</option>
              <option value="durationAsc">{t("connList.sort.durationAsc", locale)}</option>
              <option value="idDesc">{t("connList.sort.idDesc", locale)}</option>
              <option value="idAsc">{t("connList.sort.idAsc", locale)}</option>
            </select>
          </span>
        </div>
      </div>
    </Card>
  );
}
