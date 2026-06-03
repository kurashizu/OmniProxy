"use client";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";

/** Top-of-page dismissable banner that surfaces the most recent
 * `proxy-error` event (heuristic match on stderr). Cleared on
 * dismiss; also auto-cleared when the user starts/stops the proxy
 * (we listen for `proxy-state` changes in the parent and reset). */
export function ErrorBanner() {
  const err = useAppStore((s) => s.lastProxyError);
  const setErr = useAppStore((s) => s.setLastProxyError);
  const locale = useAppStore((s) => s.locale);

  if (!err) return null;

  return (
    <div
      role="alert"
      className="flex items-start gap-2 border-b border-border bg-[#7f1d1d33] px-3 py-1.5 text-[12px] text-text"
    >
      <svg viewBox="0 0 16 16" className="mt-0.5 h-3.5 w-3.5 shrink-0 fill-current text-[#f87171]" aria-hidden>
        <path d="M8 1a7 7 0 100 14A7 7 0 008 1zm0 3a1 1 0 011 1v4a1 1 0 11-2 0V5a1 1 0 011-1zm0 8a1 1 0 100 2 1 1 0 000-2z" />
      </svg>
      <div className="flex-1 min-w-0">
        <div className="font-medium text-[#fecaca]">{t("error.proxyError", locale)}</div>
        <div className="truncate text-muted" title={err.line}>{err.line}</div>
      </div>
      <button
        onClick={() => setErr(null)}
        aria-label="dismiss"
        className="shrink-0 text-muted hover:text-text"
      >
        <svg viewBox="0 0 12 12" className="h-3 w-3" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round">
          <path d="M3 3l6 6M9 3l-6 6" />
        </svg>
      </button>
    </div>
  );
}
