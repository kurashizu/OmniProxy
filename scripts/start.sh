#!/bin/bash
# Usage: sudo ./start.sh [CLIENT_PATH] [TUN2SOCKS_PATH] [-- CLIENT_ARGS...]
#   One-shot launcher: sets up TUN, routing isolation, starts client and tun2socks.
#   CLIENT_PATH: path to client binary (default: ./client)
#   TUN2SOCKS_PATH: path to tun2socks binary (default: ./tun2socks)
# Prerequisites: root privileges, client binary, tun2socks binary

set -euo pipefail

INTERFACE="tun0"
CLIENT="${1:-./client}"
TUN2SOCKS="${2:-./tun2socks}"
TABLE=20064
PRIORITY=20064
PROXY_USER=client_user

if [[ $EUID -ne 0 ]]; then
    echo "Please run with sudo" >&2
    exit 1
fi

# Create isolation user
if ! id "$PROXY_USER" &>/dev/null; then
    useradd -M -s /sbin/nologin "$PROXY_USER"
    echo "[proxy] created user: $PROXY_USER"
else
    echo "[proxy] user: $PROXY_USER"
fi
UID_CLIENT=$(id -u "$PROXY_USER")

# Get physical interface route (before tun device is up)
read -r GW IFACE < <(ip route show default | awk '!/tun/ && /^default via/ {
    for(i=1;i<=NF;i++) {
        if($i=="via") gw=$(i+1)
        if($i=="dev") dev=$(i+1)
    }
    if(gw && dev) { print gw, dev; exit }
}')
[[ -z "${GW:-}" || -z "${IFACE:-}" ]] && { echo "[proxy] failed to get IPv4 default route" >&2; exit 1; }

read -r GW6 DEV6 < <(ip -6 route show default | awk '!/tun/ && /^default via/ {
    for(i=1;i<=NF;i++) {
        if($i=="via") gw=$(i+1)
        if($i=="dev") dev=$(i+1)
    }
    if(gw && dev) { print gw, dev; exit }
}') || true

echo "[proxy] ipv4 route: via $GW dev $IFACE"
[[ -n "${GW6:-}" ]] && echo "[proxy] ipv6 route: via $GW6 dev $DEV6"

# Route isolation (client_user goes through physical interface)
ip route flush table "$TABLE" 2>/dev/null || true
ip rule del table "$TABLE" priority "$PRIORITY" 2>/dev/null || true
ip route add default via "$GW" dev "$IFACE" table "$TABLE"
ip rule add uidrange "${UID_CLIENT}-${UID_CLIENT}" table "$TABLE" priority "$PRIORITY"

ip -6 route flush table "$TABLE" 2>/dev/null || true
ip -6 rule del table "$TABLE" priority "$PRIORITY" 2>/dev/null || true
if [[ -n "${GW6:-}" && -n "${DEV6:-}" ]]; then
    ip -6 route add default via "$GW6" dev "$DEV6" table "$TABLE"
    ip -6 rule add uidrange "${UID_CLIENT}-${UID_CLIENT}" table "$TABLE" priority "$PRIORITY"
fi

# Configure TUN device
echo "[proxy] configuring TUN..."
ip link set "$INTERFACE" up 2>/dev/null || true
ip addr add 198.18.0.1/16 dev "$INTERFACE" 2>/dev/null || true
ip -6 addr add fd00::1/64 dev "$INTERFACE" 2>/dev/null || true

# Default route (metric 1, takes priority over physical interface)
ip route del default dev "$INTERFACE" proto static 2>/dev/null || true
ip -6 route del ::/0 dev "$INTERFACE" proto static 2>/dev/null || true
ip route add default dev "$INTERFACE" metric 1 proto static
ip -6 route add ::/0 dev "$INTERFACE" metric 1 proto static

echo "[proxy] ipv4 default: $(ip route show default | grep -v '^$' | head -2 | tr '\n' ' ')"
echo "[proxy] ipv6 default: $(ip -6 route show default | grep -v '^$' | head -2 | tr '\n' ' ')"

# Start client (as isolation user)
echo "[proxy] starting client..."
sudo -u "$PROXY_USER" "$CLIENT" "${@:3}" &
CLIENT_PID=$!
echo "[proxy] client pid: $CLIENT_PID"

# Wait for SOCKS5 port to be ready
for i in $(seq 1 20); do
    if ss -tnlp 2>/dev/null | grep -q ':1080'; then
        break
    fi
    sleep 0.3
done

# Start tun2socks
echo "[proxy] starting tun2socks..."
"$TUN2SOCKS" -device tun0 -proxy socks5://127.0.0.1:1080 -loglevel error &
TUN2SOCKS_PID=$!
echo "[proxy] tun2socks pid: $TUN2SOCKS_PID"

# Cleanup on exit
cleanup() {
    echo ""
    echo "[proxy] shutting down..."
    kill "$TUN2SOCKS_PID" 2>/dev/null || true
    kill "$CLIENT_PID" 2>/dev/null || true
    ip route del default dev "$INTERFACE" proto static 2>/dev/null || true
    ip -6 route del ::/0 dev "$INTERFACE" proto static 2>/dev/null || true
    ip link set "$INTERFACE" down 2>/dev/null || true
    echo "[proxy] done"
}
trap cleanup EXIT INT TERM

wait "$CLIENT_PID"