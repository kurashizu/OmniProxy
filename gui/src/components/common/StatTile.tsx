"use client";
import { clsx } from "clsx";
import type { ReactNode } from "react";

export interface StatTileProps {
  label: ReactNode;
  value: ReactNode;
  unit?: ReactNode;
  icon?: ReactNode;
  color?: string;
  className?: string;
}

export function StatTile({
  label,
  value,
  unit,
  icon,
  color = "#3b82f6",
  className,
}: StatTileProps) {
  return (
    <div
      className={clsx(
        "rounded-md border border-[#252934] bg-[#0f1115] p-3 flex flex-col gap-1",
        className,
      )}
    >
      <div className="flex items-center gap-1 text-[11px] text-[#9ca3af]">
        {icon && <span style={{ color }}>{icon}</span>}
        <span>{label}</span>
      </div>
      <div className="flex items-baseline gap-1">
        <span className="text-lg font-medium text-[#e5e7eb] tabular-nums">{value}</span>
        {unit && <span className="text-xs text-[#6b7280]">{unit}</span>}
      </div>
    </div>
  );
}
