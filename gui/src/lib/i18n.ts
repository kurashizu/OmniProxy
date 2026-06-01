import en from "./locales/en.json";
import zh from "./locales/zh.json";
import type { Locale } from "./schema";

const dicts: Record<Locale, Record<string, string>> = { en, zh };

export function t(key: string, locale: Locale, vars?: Record<string, string | number>): string {
  const d: Record<string, string> = (dicts[locale] ?? en) as Record<string, string>;
  const fallback: Record<string, string> = en as Record<string, string>;
  let s = d[key] ?? fallback[key] ?? key;
  if (vars) {
    for (const [k, v] of Object.entries(vars)) {
      s = s.replace(new RegExp(`\\{${k}\\}`, "g"), String(v));
    }
  }
  return s;
}

export function pickLocale(): Locale {
  if (typeof window === "undefined") return "en";
  try {
    const saved = localStorage.getItem("omniproxy.locale") as Locale | null;
    if (saved === "en" || saved === "zh") return saved;
  } catch {}
  // Auto-detect from browser language.
  const lang = navigator.language.toLowerCase();
  if (lang.startsWith("zh")) return "zh";
  return "en";
}

export { dicts };
