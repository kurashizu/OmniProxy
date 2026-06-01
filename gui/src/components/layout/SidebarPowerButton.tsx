"use client";
import { clsx } from "clsx";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";
import { useProxyState } from "@/hooks/useProxyState";
import { ipc } from "@/lib/ipc";
import { useBinaryPresent } from "@/hooks/useElevated";

export function SidebarPowerButton() {
  const { state, refresh } = useProxyState();
  const locale = useAppStore((s) => s.locale);
  const present = useBinaryPresent();

  const isStarting = state.state === "starting";
  const isStopping = state.state === "stopping";
  const isRunning = state.state === "running";
  const isError = state.state === "error";
  const isBusy = isStarting || isStopping;
  const disabled = isBusy || present === false;

  const onClick = async () => {
    try {
      if (isRunning) {
        await ipc.stopProxy();
      } else {
        await ipc.startProxy();
      }
      await refresh();
    } catch (e) {
      console.error(e);
    }
  };

  const label = (() => {
    if (isStarting) return t("power.starting", locale);
    if (isStopping) return t("power.stopping", locale);
    if (isRunning) return t("power.running", locale);
    if (isError) return t("power.error", locale);
    return t("power.start", locale);
  })();

  const colorClass = (() => {
    if (isError) return "bg-[#ef4444] hover:bg-[#dc2626]";
    if (isRunning) return "bg-[#ef4444] hover:bg-[#dc2626]";
    return "bg-[#10b981] hover:bg-[#059669]";
  })();

  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className={clsx(
        "flex flex-col items-center justify-center gap-1 rounded-md text-white text-xs font-medium",
        "h-20 w-12 transition-colors",
        colorClass,
        disabled && "opacity-50 cursor-not-allowed",
      )}
      title={
        present === false
          ? t("power.binaryNotFound", locale)
          : disabled
            ? ""
            : label
      }
    >
      <span className="text-lg">{isRunning ? "■" : isError ? "⚠" : "▶"}</span>
      <span className="leading-tight text-center px-0.5">
        {isRunning ? t("power.running", locale) : t("power.start", locale)}
      </span>
    </button>
  );
}
