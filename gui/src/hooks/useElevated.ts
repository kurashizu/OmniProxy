import { useEffect, useState } from "react";
import { ipc } from "@/lib/ipc";

export function useElevated(): boolean | null {
  const [elevated, setElevated] = useState<boolean | null>(null);
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const v = await ipc.isElevated();
        if (!cancelled) setElevated(v);
      } catch {
        if (!cancelled) setElevated(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);
  return elevated;
}

export function useBinaryPresent(): boolean | null {
  const [present, setPresent] = useState<boolean | null>(null);
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const v = await ipc.checkBinaryPresent();
        if (!cancelled) setPresent(v);
      } catch {
        if (!cancelled) setPresent(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);
  return present;
}
