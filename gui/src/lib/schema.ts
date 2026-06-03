export interface NodeConfig {
    name: string;
    enabled: boolean;
    server: string;
    token: string;
    socks_port: number;
    admin_port: number;
    tun_name: string;
    tun_ip: string;
    tun_ip6: string;
    tun_prefix: number;
    tun_prefix6: number;
    tun_gw: string;
    tun_gw6: string;
    phys_ip: string | null;
}

export interface GuiConfig {
    nodes: NodeConfig[];
    active_node: number;
}

export type ProxyStateKind =
    | "stopped"
    | "starting"
    | "running"
    | "stopping"
    | "error";

export interface ProxyState {
    state: ProxyStateKind;
    pid: number;
    exit_code: number | null;
    message: string | null;
}

export interface Connection {
    id: number;
    protocol: "TCP" | "UDP" | "ICMP" | string;
    target: string;
    source: string;
    duration_secs: number;
}

export interface Active {
    tcp: number;
    udp: number;
    icmp: number;
}

export interface Bytes {
    tx: number;
    rx: number;
}

export interface ServerInfo {
    server_host: string;
    server_ip: string | null;
    client_outbound_ipv4: string | null;
    client_outbound_ipv6: string | null;
    server_outbound_ipv4: string | null;
    server_outbound_ipv6: string | null;
}

export interface ClientStats {
    connected: boolean;
    uptime_secs: number;
    reconnect_count: number;
    active: Active;
    bytes: Bytes;
    socks5: string;
    server: string;
    latency_ms: number | null;
    latency_jitter_ms: number;
    server_info: ServerInfo;
    connections: Connection[];
}

export interface ProxyTun {
    name: string;
    ip: string;
}

export interface ProxyClient {
    alive: boolean;
    pid: number;
}

export interface ProxyStats {
    uptime_secs: number;
    client: ProxyClient;
    tun: ProxyTun;
    socks_port: number;
}

export interface ProxyRoute {
    destination: string;
    gateway: string;
    interface: string;
}

export type Locale = "en" | "zh";
