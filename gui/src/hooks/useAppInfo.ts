import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";

export function useAppInfo(): { version: string | null } {
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const v = await getVersion();
        if (!cancelled) setVersion(v);
      } catch { if (!cancelled) setVersion(null); }
    })();
    return () => { cancelled = true; };
  }, []);
  return { version };
}
