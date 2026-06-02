const UNITS = ["B", "KB", "MB", "GB", "TB", "PB"] as const;

export function formatBytes(
  n: number,
  opts?: { perSec?: boolean; precision?: number },
): string {
  if (!Number.isFinite(n)) return "\u2014";
  let v = Math.abs(n);
  let i = 0;
  while (v >= 1024 && i < UNITS.length - 1) { v /= 1024; i++; }
  const d = opts?.precision ?? (i === 0 ? 0 : v < 10 ? 2 : v < 100 ? 1 : 0);
  return `${v.toFixed(d)} ${UNITS[i]}${opts?.perSec ? "/s" : ""}`;
}

export function formatDuration(secs: number): string {
  if (!Number.isFinite(secs) || secs < 0) return "\u2014";
  const s = Math.floor(secs);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = s % 60;
  const p = (n: number) => n.toString().padStart(2, "0");
  if (h > 0) return `${p(h)}:${p(m)}:${p(r)}`;
  return `${p(m)}:${p(r)}`;
}

export function formatTimestamp(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => n.toString().padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

export function copyToClipboard(text: string): Promise<void> {
  if (navigator.clipboard?.writeText) return navigator.clipboard.writeText(text);
  return Promise.resolve();
}
