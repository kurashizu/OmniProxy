"use client";
import { useEffect, useRef } from "react";
import uPlot, { type AlignedData, type Options } from "uplot";

export interface ChartProps {
  data: AlignedData;
  height?: number;
  series: Options["series"];
  scales?: Options["scales"];
  axes?: Options["axes"];
  legendShow?: boolean;
}

export function Chart({
  data,
  height = 160,
  series,
  scales,
  axes,
  legendShow = true,
}: ChartProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const plotRef = useRef<uPlot | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;
    if (plotRef.current) {
      plotRef.current.destroy();
      plotRef.current = null;
    }
    const opts: Options = {
      width: containerRef.current.clientWidth || 600,
      height,
      series,
      scales: scales ?? { x: { time: false } },
      axes: axes ?? [],
      legend: { show: legendShow },
      cursor: { drag: { x: false, y: false } },
      tzDate: (ts) => new Date(ts),
    };
    plotRef.current = new uPlot(opts, data, containerRef.current);

    const ro = new ResizeObserver(() => {
      if (containerRef.current && plotRef.current) {
        plotRef.current.setSize({
          width: containerRef.current.clientWidth,
          height,
        });
      }
    });
    ro.observe(containerRef.current);
    return () => {
      ro.disconnect();
      plotRef.current?.destroy();
      plotRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    plotRef.current?.setData(data);
  }, [data]);

  return <div ref={containerRef} style={{ width: "100%", height }} />;
}
