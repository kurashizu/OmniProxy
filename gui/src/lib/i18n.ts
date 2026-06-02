import en from "./locales/en.json";
import zh from "./locales/zh.json";
import type { Locale } from "./schema";

const dicts: Record<string, Record<string, string>> = { en: en as Record<string, string>, zh: zh as Record<string, string> };

export function t(key: string, locale: Locale, vars?: Record<string, string | number>): string {
  const d = dicts[locale] ?? dicts["en"];
  let s = d[key] ?? dicts["en"][key] ?? key;
  if (vars) {
    for (const [k, v] of Object.entries(vars))
      s = s.replace(new RegExp(`\\{${k}\\}`, "g"), String(v));
  }
  return s;
}

export function pickLocale(): Locale {
  if (typeof window === "undefined") return "en";
  try {
    const saved = localStorage.getItem("omniproxy.locale") as Locale | null;
    if (saved === "en" || saved === "zh") return saved;
  } catch {}
  return navigator.language.toLowerCase().startsWith("zh") ? "zh" : "en";
}
