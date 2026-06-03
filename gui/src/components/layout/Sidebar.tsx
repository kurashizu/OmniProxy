"use client";
import { clsx } from "clsx";
import { usePathname, useRouter } from "next/navigation";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";
import { useProxyState } from "@/hooks/useProxyState";
import { ipc } from "@/lib/ipc";
import { useBinaryPresent } from "@/hooks/useElevated";

interface NavItem {
    href: string;
    icon: React.ReactNode;
    labelKey: string;
}

const NAV: NavItem[] = [
    {
        href: "/",
        icon: (
            <svg viewBox="0 0 20 20" fill="currentColor" width="18" height="18">
                <path d="M10.707 2.293a1 1 0 00-1.414 0l-7 7a1 1 0 001.414 1.414L4 10.414V17a1 1 0 001 1h2a1 1 0 001-1v-2a1 1 0 011-1h2a1 1 0 011 1v2a1 1 0 001 1h2a1 1 0 001-1v-6.586l.293.293a1 1 0 001.414-1.414l-7-7z" />
            </svg>
        ),
        labelKey: "nav.home",
    },
    {
        href: "/connections",
        icon: (
            <svg viewBox="0 0 20 20" fill="currentColor" width="18" height="18">
                <path
                    fillRule="evenodd"
                    d="M12.586 4.586a2 2 0 112.828 2.828l-3 3a2 2 0 01-2.828 0 1 1 0 00-1.414 1.414 4 4 0 005.656 0l3-3a4 4 0 00-5.656-5.656l-1.5 1.5a1 1 0 101.414 1.414l1.5-1.5zm-5 5a2 2 0 012.828 0 1 1 0 101.414-1.414 4 4 0 00-5.656 0l-3 3a4 4 0 105.656 5.656l1.5-1.5a1 1 0 10-1.414-1.414l-1.5 1.5a2 2 0 11-2.828-2.828l3-3z"
                    clipRule="evenodd"
                />
            </svg>
        ),
        labelKey: "nav.connections",
    },
    {
        href: "/routes",
        icon: (
            <svg viewBox="0 0 20 20" fill="currentColor" width="18" height="18">
                <path
                    fillRule="evenodd"
                    d="M12 1.586l-4 4v12.828l4-4V1.586zM3.707 3.293A1 1 0 002 4v10a1 1 0 00.293.707L6 18.414V5.586L3.707 3.293zm14 2L14 1.586v12.828l2.293 2.293A1 1 0 0018 16V6a1 1 0 00-.293-.707z"
                    clipRule="evenodd"
                />
            </svg>
        ),
        labelKey: "nav.routes",
    },
    {
        href: "/settings",
        icon: (
            <svg viewBox="0 0 20 20" fill="currentColor" width="18" height="18">
                <path
                    fillRule="evenodd"
                    d="M11.49 3.17c-.38-1.56-2.6-1.56-2.98 0a1.532 1.532 0 01-2.286.948c-1.372-.836-2.942.734-2.106 2.106.54.886.061 2.042-.947 2.287-1.561.379-1.561 2.6 0 2.978a1.532 1.532 0 01.947 2.287c-.836 1.372.734 2.942 2.106 2.106a1.532 1.532 0 012.287.947c.379 1.561 2.6 1.561 2.978 0a1.533 1.533 0 012.287-.947c1.372.836 2.942-.734 2.106-2.106a1.533 1.533 0 01.947-2.287c1.561-.379 1.561-2.6 0-2.978a1.532 1.532 0 01-.947-2.287c.836-1.372-.734-2.942-2.106-2.106a1.532 1.532 0 01-2.287-.947zM10 13a3 3 0 100-6 3 3 0 000 6z"
                    clipRule="evenodd"
                />
            </svg>
        ),
        labelKey: "nav.settings",
    },
];

export function Sidebar() {
    const { state, refresh } = useProxyState();
    const locale = useAppStore((s) => s.locale);
    const setLastProxyError = useAppStore((s) => s.setLastProxyError);
    const present = useBinaryPresent();
    const pathname = usePathname();
    const router = useRouter();

    const isRunning = state.state === "running";
    const isBusy = state.state === "starting" || state.state === "stopping";
    const disabled = isBusy || present === false;

    const onClick = async () => {
        // Clear any stale error banner from the previous run.
        setLastProxyError(null);
        try {
            if (isRunning) await ipc.stopProxy();
            else await ipc.startProxy();
            await refresh();
        } catch (e) {
            console.error(e);
        }
    };

    return (
        <aside className="w-44 flex-none flex flex-col border-r border-border bg-card h-full">
            <nav className="flex-1 min-h-0 overflow-y-auto flex flex-col gap-0.5 py-3 px-2">
                {NAV.map((item) => {
                    const active =
                        item.href === "/"
                            ? pathname === "/"
                            : pathname.startsWith(item.href);
                    return (
                        <button
                            key={item.href}
                            onClick={() => router.push(item.href)}
                            className={clsx(
                                "flex items-center gap-2.5 rounded-md px-3 py-2 text-[13px] transition-colors",
                                active
                                    ? "bg-primary/20 text-text"
                                    : "text-muted hover:bg-border hover:text-text",
                            )}
                        >
                            <span className="shrink-0">{item.icon}</span>
                            <span>{t(item.labelKey, locale)}</span>
                        </button>
                    );
                })}
            </nav>
            <div className="border-t border-border px-3 py-2.5">
                <button
                    onClick={onClick}
                    disabled={disabled}
                    className={clsx(
                        "flex w-full items-center justify-center gap-2 rounded-md text-white text-[13px] font-medium h-8 transition-colors",
                        isRunning
                            ? "bg-danger hover:bg-[#dc2626]"
                            : "bg-success hover:bg-[#059669]",
                        disabled && "opacity-50 cursor-not-allowed",
                    )}
                >
                    <span className="text-xs">
                        {isRunning ? "\u25A0" : "\u25B6"}
                    </span>
                    <span>
                        {state.state === "starting"
                            ? t("power.starting", locale)
                            : state.state === "stopping"
                              ? t("power.stopping", locale)
                              : isRunning
                                ? t("power.running", locale)
                                : t("power.start", locale)}
                    </span>
                </button>
            </div>
        </aside>
    );
}
