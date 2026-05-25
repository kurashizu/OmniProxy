#!/bin/bash
# Usage: ./run_client.sh [--help] [-- SERVER_ARGS...]
#   Routes client traffic via physical interface using uid-based policy.
#   Creates 'client_user' if missing. Requires root to set up routing.
# Prerequisites: client binary, sudo access

set -euo pipefail
TABLE=20064
PRIORITY=20064
USER=client_user

# Create user if not exists
if ! id "$USER" &>/dev/null; then
    useradd -M -s /sbin/nologin "$USER"
    echo "[run_client] created user: $USER"
else
    echo "[run_client] user already exists: $USER"
fi

# Get IPv4 default route (exclude tun)
read -r GW IFACE < <(ip route show default | awk '!/tun/ && /^default via/ {
    for(i=1;i<=NF;i++) {
        if($i=="via") gw=$(i+1)
        if($i=="dev") dev=$(i+1)
    }
    if(gw && dev) { print gw, dev; exit }
}')
[[ -z "$GW" || -z "$IFACE" ]] && { echo "[run_client] failed to get IPv4 default route" >&2; exit 1; }

# Get IPv6 default route (exclude tun)
read -r GW6 DEV6 < <(ip -6 route show default | awk '!/tun/ && /^default via/ {
    for(i=1;i<=NF;i++) {
        if($i=="via") gw=$(i+1)
        if($i=="dev") dev=$(i+1)
    }
    if(gw && dev) { print gw, dev; exit }
}')

UID_CLIENT=$(id -u "$USER")
echo "[run_client] ipv4 route: via $GW dev $IFACE"
echo "[run_client] uid: $UID_CLIENT, table: $TABLE"

# Idempotent: flush then add (IPv4)
ip route flush table "$TABLE" 2>/dev/null || true
ip rule del table "$TABLE" priority "$PRIORITY" 2>/dev/null || true
ip route add default via "$GW" dev "$IFACE" table "$TABLE"
ip rule add uidrange "${UID_CLIENT}-${UID_CLIENT}" table "$TABLE" priority "$PRIORITY"

# IPv6 (if available)
ip -6 route flush table "$TABLE" 2>/dev/null || true
ip -6 rule del table "$TABLE" priority "$PRIORITY" 2>/dev/null || true
if [[ -n "${GW6:-}" && -n "${DEV6:-}" ]]; then
    ip -6 route add default via "$GW6" dev "$DEV6" table "$TABLE"
    ip -6 rule add uidrange "${UID_CLIENT}-${UID_CLIENT}" table "$TABLE" priority "$PRIORITY"
    echo "[run_client] ipv6 route: via $GW6 dev $DEV6"
else
    echo "[run_client] no ipv6 default route found, skipping"
fi

echo "[run_client] ready, starting client..."
exec sudo -u "$USER" "$@"