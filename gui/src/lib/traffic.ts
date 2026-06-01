// Sample buffer for traffic-rate calculations.

export interface TrafficSample {
  t: number; // ms
  up: number; // bytes/s
  down: number; // bytes/s
  txTotal: number;
  rxTotal: number;
}

export class TrafficRingBuffer {
  private buf: TrafficSample[] = [];
  private cap: number;

  constructor(capacity = 300) {
    this.cap = capacity;
  }

  push(s: TrafficSample) {
    this.buf.push(s);
    if (this.buf.length > this.cap) {
      this.buf.shift();
    }
  }

  values(): TrafficSample[] {
    return this.buf;
  }

  latest(): TrafficSample | undefined {
    return this.buf[this.buf.length - 1];
  }

  clear() {
    this.buf = [];
  }
}

/**
 * Compute a traffic sample given two consecutive `bytes` snapshots.
 * Returns null if the time delta is too small (first sample).
 */
export function diffTraffic(
  prev: { tx: number; rx: number; t: number } | null,
  curr: { tx: number; rx: number; t: number },
): { up: number; down: number; txTotal: number; rxTotal: number } | null {
  if (!prev) return null;
  const dt = (curr.t - prev.t) / 1000;
  if (dt <= 0) return null;
  const up = Math.max(0, curr.tx - prev.tx) / dt;
  const down = Math.max(0, curr.rx - prev.rx) / dt;
  return { up, down, txTotal: curr.tx, rxTotal: curr.rx };
}
