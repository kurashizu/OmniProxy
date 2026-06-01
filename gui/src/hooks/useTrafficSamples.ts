import { useEffect, useRef, useState } from "react";
import { TrafficRingBuffer, diffTraffic, type TrafficSample } from "@/lib/traffic";
import type { ClientStats } from "@/lib/schema";

/**
 * Maintains a ring buffer of traffic rate samples derived from successive
 * `/stats` snapshots. The buffer's `latest()` value is the current up/down
 * rate in bytes/s.
 */
export function useTrafficSamples(stats: ClientStats | null | undefined) {
  const buf = useRef<TrafficRingBuffer>(new TrafficRingBuffer(300));
  const [tick, setTick] = useState(0);
  const prev = useRef<{ tx: number; rx: number; t: number } | null>(null);

  useEffect(() => {
    if (!stats) return;
    const curr = {
      tx: stats.bytes.tx,
      rx: stats.bytes.rx,
      t: Date.now(),
    };
    const sample = diffTraffic(prev.current, curr);
    prev.current = curr;
    if (sample) {
      buf.current.push({
        t: curr.t,
        up: sample.up,
        down: sample.down,
        txTotal: sample.txTotal,
        rxTotal: sample.rxTotal,
      });
    }
    setTick((n) => n + 1);
  }, [stats]);

  return {
    samples: buf.current.values(),
    latest: buf.current.latest() as TrafficSample | undefined,
    clear: () => {
      buf.current.clear();
      setTick((n) => n + 1);
    },
    _tick: tick,
  };
}
