"use client";
import { Card } from "@/components/common/Card";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";
import type { ProxyRoute } from "@/lib/schema";

export function RoutesTableCard({ routes }: { routes: ProxyRoute[] }) {
  const locale = useAppStore((s) => s.locale);
  const paused = useAppStore((s) => s.routesPaused);
  const setPaused = useAppStore((s) => s.setRoutesPaused);
  return (
    <Card
      title={`${t("routes.title", locale)} (${routes.length})`}
      extra={
        <button onClick={() => setPaused(!paused)}
          className="rounded border border-border bg-surface px-2 py-0.5 text-muted hover:text-text hover:border-primary text-xs"
        >
          {paused ? t("routes.resume", locale) : t("routes.pause", locale)}
        </button>
      }
      className="h-full"
      bodyClassName="p-0 min-h-0"
    >
      <div className="h-full overflow-auto">
        {routes.length === 0 ? (
          <div className="px-4 py-8 text-center text-sm text-[#6b7280]">{t("routes.empty", locale)}</div>
        ) : (
          <table className="w-full text-sm">
            <thead className="text-[11px] uppercase text-muted">
              <tr>
                <th className="px-3 py-2 text-left">{t("routes.destination", locale)}</th>
                <th className="px-3 py-2 text-left">{t("routes.gateway", locale)}</th>
                <th className="px-3 py-2 text-left">{t("routes.interface", locale)}</th>
              </tr>
            </thead>
            <tbody>
              {routes.map((r, i) => (
                <tr key={i} className="border-t border-border">
                  <td className="px-3 py-1.5 text-text tabular-nums">{r.destination}</td>
                  <td className="px-3 py-1.5 text-muted tabular-nums">{r.gateway}</td>
                  <td className="px-3 py-1.5 text-muted">{r.interface}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </Card>
  );
}
