export const theme = {
  colors: {
    bg: "#0f1115",
    card: "#171a21",
    cardHover: "#1d212a",
    border: "#252934",
    borderHover: "#2f3543",
    text: "#e5e7eb",
    textMuted: "#9ca3af",
    textDim: "#6b7280",
    primary: "#3b82f6",
    primaryBg: "rgba(59, 130, 246, 0.2)",
    success: "#10b981",
    warning: "#f59e0b",
    danger: "#ef4444",
    disabled: "#6b7280",
    tcp: "#3b82f6",
    udp: "#10b981",
    icmp: "#f59e0b",
    up: "#3b82f6",
    down: "#a855f7",
    latencyGood: "#10b981",
    latencyMid: "#f59e0b",
    latencyBad: "#ef4444",
  },
  radius: {
    sm: "4px",
    md: "6px",
    lg: "8px",
  },
  sidebar: {
    width: "72px",
  },
} as const;

export type Theme = typeof theme;
