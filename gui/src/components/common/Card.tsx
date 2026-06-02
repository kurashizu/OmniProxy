"use client";
import { clsx } from "clsx";
import type { ReactNode } from "react";

export function Card({ title, extra, className, bodyClassName, children }: {
  title?: ReactNode; extra?: ReactNode; className?: string; bodyClassName?: string; children: ReactNode;
}) {
  return (
    <div className={clsx("rounded-lg border border-border bg-card flex flex-col overflow-hidden", className)}>
      {(title || extra) && (
        <div className="flex items-center justify-between border-b border-border px-3 py-2 text-muted">
          <div className="text-[11px] uppercase tracking-[0.18em]">{title}</div>
          <div className="flex items-center gap-2">{extra}</div>
        </div>
      )}
      <div className={clsx("p-3 flex-1 min-h-0", bodyClassName)}>{children}</div>
    </div>
  );
}
