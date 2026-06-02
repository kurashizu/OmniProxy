export interface TrafficSample {
  t: number;
  up: number;
  down: number;
  txTotal: number;
  rxTotal: number;
}

export class TrafficRingBuffer {
  private buf: TrafficSample[] = [];
  private cap: number;

  constructor(cap: number) { this.cap = cap; }

  push(s: TrafficSample): void {
    this.buf.push(s);
    if (this.buf.length > this.cap) this.buf.shift();
  }

  values(): TrafficSample[] { return this.buf; }

  latest(): TrafficSample | undefined {
    return this.buf[this.buf.length - 1];
  }

  clear(): void { this.buf = []; }
}

export function diffTraffic(
  prev: { tx: number; rx: number; t: number } | null,
  curr: { tx: number; rx: number; t: number },
): { up: number; down: number; txTotal: number; rxTotal: number } | null {
  if (!prev || prev.t === curr.t) return null;
  const dt = (curr.t - prev.t) / 1000;
  if (dt <= 0) return null;
  return {
    up: Math.max(0, (curr.tx - prev.tx) / dt),
    down: Math.max(0, (curr.rx - prev.rx) / dt),
    txTotal: curr.tx,
    rxTotal: curr.rx,
  };
}
