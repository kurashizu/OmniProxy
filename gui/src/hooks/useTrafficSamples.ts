import { useEffect, useRef, useState } from "react";
import { TrafficRingBuffer, diffTraffic } from "@/lib/traffic";
import type { ClientStats } from "@/lib/schema";

export function useTrafficSamples(stats: ClientStats | null | undefined) {
  const buf = useRef(new TrafficRingBuffer(300));
  const [, setTick] = useState(0);
  const prev = useRef<{ tx: number; rx: number; t: number } | null>(null);

  useEffect(() => {
    if (!stats) return;
    const curr = { tx: stats.bytes.tx, rx: stats.bytes.rx, t: Date.now() };
    const sample = diffTraffic(prev.current, curr);
    prev.current = curr;
    if (sample) buf.current.push({ t: curr.t, ...sample });
    setTick((n) => n + 1);
  }, [stats]);

  return {
    samples: buf.current.values(),
    latest: buf.current.latest(),
    clear: () => { buf.current.clear(); setTick((n) => n + 1); },
  };
}
