// Formatting helpers.

const UNITS_1024 = ["B", "KB", "MB", "GB", "TB", "PB"] as const;
const UNITS_SI = ["B", "KB", "MB", "GB", "TB", "PB"] as const;

export function formatBytes(
  n: number,
  opts?: { perSec?: boolean; precision?: number },
): string {
  if (!Number.isFinite(n)) return "—";
  const sign = n < 0 ? "-" : "";
  let v = Math.abs(n);
  let i = 0;
  while (v >= 1024 && i < UNITS_1024.length - 1) {
    v /= 1024;
    i++;
  }
  const decimals =
    opts?.precision ??
    (i === 0 ? 0 : v < 10 ? 2 : v < 100 ? 1 : 0);
  return `${sign}${v.toFixed(decimals)} ${UNITS_1024[i]}${opts?.perSec ? "/s" : ""}`;
}

export function formatBytesAuto(
  n: number,
  opts?: { perSec?: boolean },
): string {
  if (!Number.isFinite(n) || n === 0)
    return `0 ${UNITS_SI[0]}${opts?.perSec ? "/s" : ""}`;
  const exp = Math.min(
    Math.floor(Math.log10(Math.abs(n)) / 3),
    UNITS_SI.length - 1,
  );
  const v = n / Math.pow(1000, exp);
  return `${v.toFixed(v < 10 ? 2 : v < 100 ? 1 : 0)} ${UNITS_SI[exp]}${
    opts?.perSec ? "/s" : ""
  }`;
}

export function formatDuration(secs: number): string {
  if (!Number.isFinite(secs) || secs < 0) return "—";
  const s = Math.floor(secs);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = s % 60;
  const pad = (n: number) => n.toString().padStart(2, "0");
  if (h > 0) return `${pad(h)}:${pad(m)}:${pad(r)}`;
  return `${pad(m)}:${pad(r)}`;
}

export function formatTimestamp(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

export function formatPort(port: number): string {
  return port > 0 ? port.toString() : "—";
}

export function copyToClipboard(text: string): Promise<void> {
  if (navigator.clipboard?.writeText) {
    return navigator.clipboard.writeText(text);
  }
  return Promise.resolve();
}
