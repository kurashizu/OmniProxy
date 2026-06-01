"use client";
import { clsx } from "clsx";
import { Card } from "@/components/common/Card";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";

export function ConnectionSearchCard() {
  const locale = useAppStore((s) => s.locale);
  const filter = useAppStore((s) => s.connectionFilter);
  const setFilter = useAppStore((s) => s.setConnectionFilter);
  const paused = useAppStore((s) => s.connsPaused);
  const setPaused = useAppStore((s) => s.setConnsPaused);
  const onRefresh = useAppStore.getState; // unused

  return (
    <Card
      title={t("connList.title", locale)}
      extra={
        <div className="flex items-center gap-2 text-xs">
          <button
            onClick={() => setPaused(!paused)}
            className="rounded border border-[#252934] bg-[#0f1115] px-2 py-0.5 text-[#9ca3af] hover:text-[#e5e7eb] hover:border-[#3b82f6]"
          >
            {paused ? t("connList.resume", locale) : t("connList.pause", locale)}
          </button>
        </div>
      }
    >
      <div className="flex flex-col gap-2">
        <div className="flex items-center gap-2">
          <span className="text-xs text-[#9ca3af]">🔍</span>
          <input
            value={filter.search}
            onChange={(e) => setFilter({ search: e.target.value })}
            placeholder={t("connList.search", locale)}
            className="flex-1"
          />
        </div>
        <div className="flex flex-wrap items-center gap-3 text-xs">
          <span className="text-[#9ca3af]">{t("connList.filter.protocol", locale)}:</span>
          {(["TCP", "UDP", "ICMP"] as const).map((p) => (
            <label key={p} className="flex items-center gap-1 cursor-pointer">
              <input
                type="checkbox"
                checked={filter.protocol[p]}
                onChange={() =>
                  setFilter({
                    protocol: { ...filter.protocol, [p]: !filter.protocol[p] },
                  })
                }
              />
              <span>{p}</span>
            </label>
          ))}
          <span className="ml-auto flex items-center gap-2">
            <span className="text-[#9ca3af]">{t("connList.sort", locale)}:</span>
            <select
              value={filter.sort}
              onChange={(e) =>
                setFilter({ sort: e.target.value as typeof filter.sort })
              }
              className="text-xs"
            >
              <option value="durationDesc">
                {t("connList.sort.durationDesc", locale)}
              </option>
              <option value="durationAsc">
                {t("connList.sort.durationAsc", locale)}
              </option>
              <option value="idDesc">{t("connList.sort.idDesc", locale)}</option>
              <option value="idAsc">{t("connList.sort.idAsc", locale)}</option>
            </select>
          </span>
        </div>
      </div>
    </Card>
  );
}
