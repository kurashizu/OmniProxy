#!/bin/bash
# 按照用户 UID 进行路由，确保客户端流量走物理网络接口
set -euo pipefail
TABLE=20064
PRIORITY=20064
USER=client_user

# 创建用户（已存在则跳过）
if ! id "$USER" &>/dev/null; then
    useradd -M -s /sbin/nologin "$USER"
    echo "[run-physical] created user: $USER"
else
    echo "[run-physical] user already exists: $USER"
fi

# 获取 IPv4 默认路由（排除 tun）
read -r GW IFACE < <(ip route show default | awk '!/tun/ && /^default via/ {
    for(i=1;i<=NF;i++) {
        if($i=="via") gw=$(i+1)
        if($i=="dev") dev=$(i+1)
    }
    if(gw && dev) { print gw, dev; exit }
}')
[[ -z "$GW" || -z "$IFACE" ]] && { echo "[run-physical] 无法获取 IPv4 默认路由" >&2; exit 1; }

# 获取 IPv6 默认路由（排除 tun）
read -r GW6 DEV6 < <(ip -6 route show default | awk '!/tun/ && /^default via/ {
    for(i=1;i<=NF;i++) {
        if($i=="via") gw=$(i+1)
        if($i=="dev") dev=$(i+1)
    }
    if(gw && dev) { print gw, dev; exit }
}')

UID_CLIENT=$(id -u "$USER")
echo "[run-physical] ipv4 route: via $GW dev $IFACE"
echo "[run-physical] uid: $UID_CLIENT, table: $TABLE"

# 幂等：先清理再添加（IPv4）
ip route flush table "$TABLE" 2>/dev/null || true
ip rule del table "$TABLE" priority "$PRIORITY" 2>/dev/null || true
ip route add default via "$GW" dev "$IFACE" table "$TABLE"
ip rule add uidrange "${UID_CLIENT}-${UID_CLIENT}" table "$TABLE" priority "$PRIORITY"

# IPv6（有则配置）
ip -6 route flush table "$TABLE" 2>/dev/null || true
ip -6 rule del table "$TABLE" priority "$PRIORITY" 2>/dev/null || true
if [[ -n "${GW6:-}" && -n "${DEV6:-}" ]]; then
    ip -6 route add default via "$GW6" dev "$DEV6" table "$TABLE"
    ip -6 rule add uidrange "${UID_CLIENT}-${UID_CLIENT}" table "$TABLE" priority "$PRIORITY"
    echo "[run-physical] ipv6 route: via $GW6 dev $DEV6"
else
    echo "[run-physical] no ipv6 default route found, skipping"
fi

echo "[run-physical] ready, starting client..."
exec sudo -u "$USER" "$@"
