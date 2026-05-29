#!/bin/bash
# =============================================================================
# proxy.sh — TUN-based transparent proxy launcher
# =============================================================================
#
# DESCRIPTION:
#   Sets up a full transparent proxy stack on Linux:
#     1. Creates an isolated routing table for the proxy client user
#        (prevents routing loops — the client itself bypasses the TUN)
#     2. Starts the SOCKS5 proxy client
#     3. Configures the TUN interface with routes to intercept all traffic
#   On exit (Ctrl+C or error), all routes and processes are cleaned up.
#
# USAGE:
#   sudo ./proxy.sh [OPTIONS] -- [CLIENT ARGS...]
#
# OPTIONS:
#   -c, --client PATH       Path to the socks5 client binary  (default: ./client)
#   -i, --iface NAME        TUN interface name                 (default: tun0)
#   -p, --port PORT         SOCKS5 listen port                 (default: 1080)
#   -u, --user USER         Isolated user for the client       (default: client_user)
#       --table NUM         Routing table ID                   (default: 20064)
#       --priority NUM      ip rule priority                   (default: 20064)
#   -h, --help              Show this help message and exit
#
# EXAMPLES:
#   # Basic usage — client in current directory
#   sudo ./proxy.sh -- --server example.com --token secret
#
#   # Custom binary path
#   sudo ./proxy.sh -c /usr/local/bin/client \
#        -- --server example.com --token secret
#
#   # Custom TUN interface name
#   sudo ./proxy.sh -i tun1 -- --server example.com --token secret
#
# REQUIREMENTS:
#   - Run as root (sudo)
#   - iproute2 (ip command)
#   - Linux kernel with TUN/TAP support (/dev/net/tun)
#
# =============================================================================

set -euo pipefail

# ── Defaults ──────────────────────────────────────────────────────────────────
CLIENT="./client"
INTERFACE="tun0"
SOCKS_PORT="1080"
PROXY_USER="client_user"
TABLE=20064
PRIORITY=20064

# ── Colors ────────────────────────────────────────────────────────────────────
if [[ -t 1 ]]; then
    RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
    CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'
else
    RED=''; GREEN=''; YELLOW=''; CYAN=''; BOLD=''; RESET=''
fi

log()  { echo -e "${CYAN}[proxy]${RESET} $*"; }
ok()   { echo -e "${GREEN}[proxy]${RESET} $*"; }
warn() { echo -e "${YELLOW}[proxy]${RESET} $*"; }
die()  { echo -e "${RED}[proxy] ERROR:${RESET} $*" >&2; exit 1; }

# ── Help ──────────────────────────────────────────────────────────────────────
usage() {
    sed -n '/^# DESCRIPTION:/,/^# =\+$/p' "$0" | sed 's/^# \?//' | head -n -1
    exit 0
}

# ── Argument parsing ──────────────────────────────────────────────────────────
CLIENT_ARGS=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        -h|--help)       usage ;;
        -c|--client)     CLIENT="$2";      shift 2 ;;
        -i|--iface)      INTERFACE="$2";   shift 2 ;;
        -p|--port)       SOCKS_PORT="$2";  shift 2 ;;
        -u|--user)       PROXY_USER="$2";  shift 2 ;;
        --table)         TABLE="$2";       shift 2 ;;
        --priority)      PRIORITY="$2";    shift 2 ;;
        --)              shift; CLIENT_ARGS=("$@"); break ;;
        *)               die "Unknown option: $1  (use -h for help)" ;;
    esac
done

# ── Sanity checks ─────────────────────────────────────────────────────────────
[[ $EUID -eq 0 ]] || die "This script must be run with sudo."

[[ -x "$CLIENT" ]] || die "Client binary not found or not executable: $CLIENT"

command -v ip   &>/dev/null || die "'ip' command not found. Install iproute2."
command -v ss   &>/dev/null || die "'ss' command not found. Install iproute2."
[[ -c /dev/net/tun ]] || die "/dev/net/tun not available. Is TUN/TAP enabled in your kernel?"

# ── State for cleanup ─────────────────────────────────────────────────────────
CLIENT_PID=""
TUN_CONFIGURED=false
RULES_CONFIGURED=false

cleanup() {
    local exit_code=$?
    echo ""
    log "Shutting down..."

    [[ -n "$CLIENT_PID" ]] && kill "$CLIENT_PID" 2>/dev/null && \
        log "Stopped client (pid $CLIENT_PID)"

    if [[ "$TUN_CONFIGURED" == true ]]; then
        ip route del default dev "$INTERFACE" proto static 2>/dev/null || true
        ip -6 route del ::/0 dev "$INTERFACE" proto static 2>/dev/null || true
        ip link set "$INTERFACE" down 2>/dev/null || true
        log "Removed TUN routes and brought down $INTERFACE"
    fi

    if [[ "$RULES_CONFIGURED" == true ]]; then
        ip rule del table "$TABLE" priority "$PRIORITY" 2>/dev/null || true
        ip -6 rule del table "$TABLE" priority "$PRIORITY" 2>/dev/null || true
        ip route flush table "$TABLE" 2>/dev/null || true
        ip -6 route flush table "$TABLE" 2>/dev/null || true
        log "Removed routing rules (table $TABLE)"
    fi

    ok "Cleanup complete."
    exit $exit_code
}
trap cleanup EXIT INT TERM

# ── Create isolated user ──────────────────────────────────────────────────────
if ! id "$PROXY_USER" &>/dev/null; then
    useradd -M -s /sbin/nologin "$PROXY_USER" \
        || die "Failed to create user: $PROXY_USER"
    ok "Created user: $PROXY_USER"
else
    log "Using existing user: $PROXY_USER"
fi
UID_CLIENT=$(id -u "$PROXY_USER")

# ── Read physical routes BEFORE TUN takes over ────────────────────────────────
log "Detecting physical network routes..."

GW=""
IFACE_PHYS=""
while read -r line; do
    if [[ "$line" =~ ^default\ via ]]; then
        [[ "$line" =~ tun ]] && continue
        GW=$(echo "$line"    | awk '{for(i=1;i<=NF;i++) if($i=="via") print $(i+1)}')
        IFACE_PHYS=$(echo "$line" | awk '{for(i=1;i<=NF;i++) if($i=="dev") print $(i+1)}')
        [[ -n "$GW" && -n "$IFACE_PHYS" ]] && break
    fi
done < <(ip route show default)

[[ -z "$GW" || -z "$IFACE_PHYS" ]] && \
    die "Could not detect IPv4 default gateway. Are you connected to a network?"

GW6=""
DEV6=""
while read -r line; do
    if [[ "$line" =~ ^default\ via ]]; then
        [[ "$line" =~ tun ]] && continue
        GW6=$(echo "$line"  | awk '{for(i=1;i<=NF;i++) if($i=="via") print $(i+1)}')
        DEV6=$(echo "$line" | awk '{for(i=1;i<=NF;i++) if($i=="dev") print $(i+1)}')
        [[ -n "$GW6" && -n "$DEV6" ]] && break
    fi
done < <(ip -6 route show default) || true

log "IPv4 gateway: $GW via $IFACE_PHYS"
[[ -n "$GW6" ]] && log "IPv6 gateway: $GW6 via $DEV6" || warn "No IPv6 gateway found — IPv6 routing isolation skipped"

# ── Configure routing isolation for client user ───────────────────────────────
log "Setting up routing isolation for uid $UID_CLIENT (table $TABLE)..."

ip route flush table "$TABLE" 2>/dev/null || true
ip -6 route flush table "$TABLE" 2>/dev/null || true
ip rule del table "$TABLE" priority "$PRIORITY" 2>/dev/null || true
ip -6 rule del table "$TABLE" priority "$PRIORITY" 2>/dev/null || true

ip route add default via "$GW" dev "$IFACE_PHYS" table "$TABLE" \
    || die "Failed to add IPv4 route to table $TABLE"
ip rule add uidrange "${UID_CLIENT}-${UID_CLIENT}" table "$TABLE" priority "$PRIORITY" \
    || die "Failed to add IPv4 rule for uid $UID_CLIENT"

if [[ -n "$GW6" && -n "$DEV6" ]]; then
    ip -6 route add default via "$GW6" dev "$DEV6" table "$TABLE" \
        || warn "Failed to add IPv6 route to table $TABLE (non-fatal)"
    ip -6 rule add uidrange "${UID_CLIENT}-${UID_CLIENT}" table "$TABLE" priority "$PRIORITY" \
        || warn "Failed to add IPv6 rule for uid $UID_CLIENT (non-fatal)"
fi

RULES_CONFIGURED=true
ok "Routing isolation ready"

# ── Start proxy client ────────────────────────────────────────────────────────
log "Starting client as $PROXY_USER: $CLIENT ${CLIENT_ARGS[*]:-}"
sudo -u "$PROXY_USER" "$CLIENT" "${CLIENT_ARGS[@]:-}" &
CLIENT_PID=$!

# Verify the client process started
sleep 0.3
kill -0 "$CLIENT_PID" 2>/dev/null \
    || die "Client process exited immediately. Check your client arguments."

# Wait for SOCKS5 port to become available (up to 10 seconds)
log "Waiting for SOCKS5 on port $SOCKS_PORT..."
WAIT_OK=false
for i in $(seq 1 33); do
    if ss -tnlp 2>/dev/null | grep -q ":${SOCKS_PORT}"; then
        WAIT_OK=true
        break
    fi
    kill -0 "$CLIENT_PID" 2>/dev/null \
        || die "Client process died while waiting for SOCKS5 port."
    sleep 0.3
done
[[ "$WAIT_OK" == true ]] || die "SOCKS5 port $SOCKS_PORT did not open within 10 seconds."
ok "SOCKS5 listening on :$SOCKS_PORT"

# ── Wait for TUN device to appear ────────────────────────────────────────────
# TUN creation is now handled by the proxy binary's built-in forwarder.
# Wait for the interface to appear (up to 10 seconds).
log "Waiting for $INTERFACE device (created by proxy forwarder)..."
WAIT_OK=false
for i in $(seq 1 50); do
    if ip link show "$INTERFACE" &>/dev/null; then
        WAIT_OK=true
        break
    fi
    sleep 0.2
done
[[ "$WAIT_OK" == true ]] || die "$INTERFACE did not appear within 10 seconds."

# ── Configure TUN interface ───────────────────────────────────────────────────
log "Configuring $INTERFACE..."
ip link set "$INTERFACE" up \
    || die "Failed to bring up $INTERFACE"
ip addr add 198.18.0.1/16 dev "$INTERFACE" 2>/dev/null \
    || warn "IPv4 address already set on $INTERFACE (non-fatal)"
ip -6 addr add fd00::1/64 dev "$INTERFACE" 2>/dev/null \
    || warn "IPv6 address already set on $INTERFACE (non-fatal)"

ip route del default dev "$INTERFACE" proto static 2>/dev/null || true
ip -6 route del ::/0 dev "$INTERFACE" proto static 2>/dev/null || true
ip route add default dev "$INTERFACE" metric 1 proto static \
    || die "Failed to add IPv4 default route via $INTERFACE"
ip -6 route add ::/0 dev "$INTERFACE" metric 1 proto static \
    || warn "Failed to add IPv6 default route via $INTERFACE (non-fatal)"

TUN_CONFIGURED=true

# ── Ready ─────────────────────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}${GREEN}All traffic is now routed through the proxy.${RESET}"
echo -e "  IPv4 default: $(ip route show default | grep "$INTERFACE" | head -1)"
echo -e "  IPv6 default: $(ip -6 route show default | grep "$INTERFACE" | head -1)"
echo -e "  Client pid:   $CLIENT_PID"
echo -e "  Press ${BOLD}Ctrl+C${RESET} to stop and clean up."
echo ""

wait "$CLIENT_PID"
