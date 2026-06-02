"use client";
import { useEffect, useRef } from "react";
import uPlot, { type AlignedData, type Options } from "uplot";

export function Chart({ data, height = 160, series, scales, axes, legendShow = true }: {
  data: AlignedData; height?: number; series: Options["series"];
  scales?: Options["scales"]; axes?: Options["axes"]; legendShow?: boolean;
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const plotRef = useRef<uPlot | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;
    plotRef.current?.destroy();
    const opts: Options = {
      width: containerRef.current.clientWidth || 600, height: containerRef.current.clientHeight || height,
      series, legend: { show: legendShow },
      scales: scales ?? { x: { time: false } },
      axes: axes ?? [],
      cursor: { drag: { x: false, y: false } },
      tzDate: (ts) => new Date(ts),
    };
    plotRef.current = new uPlot(opts, data, containerRef.current);
    const ro = new ResizeObserver(() => {
      if (containerRef.current && plotRef.current)
        plotRef.current.setSize({ width: containerRef.current.clientWidth, height: containerRef.current.clientHeight || height });
    });
    ro.observe(containerRef.current);
    return () => { ro.disconnect(); plotRef.current?.destroy(); plotRef.current = null; };
  }, []);

  useEffect(() => { plotRef.current?.setData(data); }, [data]);

  return <div ref={containerRef} className="w-full h-full min-h-0" style={{ minHeight: height }} />;
}
