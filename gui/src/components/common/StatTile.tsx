"use client";
import { clsx } from "clsx";
import type { ReactNode } from "react";

export function StatTile({ label, value, icon, className }: {
  label: ReactNode; value: ReactNode; icon?: ReactNode; className?: string;
}) {
  return (
    <div className={clsx("rounded-md border border-border bg-surface p-2.5 flex flex-col gap-1", className)}>
      <div className="flex items-center gap-1 text-[11px] text-muted">
        {icon && <span>{icon}</span>}
        <span>{label}</span>
      </div>
      <span className="text-[15px] font-medium text-text tabular-nums">{value}</span>
    </div>
  );
}
