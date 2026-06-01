"use client";
import { clsx } from "clsx";
import type { ReactNode } from "react";

export interface CardProps {
  title?: ReactNode;
  extra?: ReactNode;
  className?: string;
  bodyClassName?: string;
  children: ReactNode;
}

export function Card({ title, extra, className, bodyClassName, children }: CardProps) {
  return (
    <div
      className={clsx(
        "rounded-lg border border-[#252934] bg-[#171a21] flex flex-col",
        className,
      )}
    >
      {(title || extra) && (
        <div className="flex items-center justify-between border-b border-[#252934] px-4 py-2 text-[#9ca3af]">
          <div className="text-xs uppercase tracking-wider">{title}</div>
          <div className="flex items-center gap-2">{extra}</div>
        </div>
      )}
      <div className={clsx("p-4 flex-1 min-h-0", bodyClassName)}>{children}</div>
    </div>
  );
}
