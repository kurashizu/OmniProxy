"use client";
import { usePathname, useRouter } from "next/navigation";
import { clsx } from "clsx";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";

interface NavItem {
  href: string;
  icon: string;
  labelKey: string;
}

const NAV: NavItem[] = [
  { href: "/", icon: "🐾", labelKey: "nav.home" },
  { href: "/connections", icon: "🔗", labelKey: "nav.connections" },
  { href: "/routes", icon: "🗺", labelKey: "nav.routes" },
  { href: "/logs", icon: "📋", labelKey: "nav.logs" },
  { href: "/settings", icon: "⚙", labelKey: "nav.settings" },
];

export function SidebarNav() {
  const pathname = usePathname();
  const router = useRouter();
  const locale = useAppStore((s) => s.locale);
  return (
    <nav className="flex flex-col items-center gap-1 py-2">
      {NAV.map((item) => {
        const active = item.href === "/" ? pathname === "/" : pathname.startsWith(item.href);
        return (
          <button
            key={item.href}
            onClick={() => router.push(item.href)}
            title={t(item.labelKey, locale)}
            className={clsx(
              "relative flex h-12 w-12 items-center justify-center rounded-md text-xl transition-colors",
              active
                ? "bg-[#3b82f6]/20 text-[#e5e7eb] border-l-2 border-[#3b82f6]"
                : "text-[#9ca3af] hover:bg-[#252934] hover:text-[#e5e7eb]",
            )}
          >
            {item.icon}
          </button>
        );
      })}
    </nav>
  );
}
