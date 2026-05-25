#!/bin/bash
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

# 获取当前默认路由的网关和网卡
read -r GW IFACE < <(ip route show default | awk '/^default/ {
    for(i=1;i<=NF;i++) {
        if($i=="via") gw=$(i+1)
        if($i=="dev") dev=$(i+1)
    }
    print gw, dev
    exit
}')

[[ -z "$GW" || -z "$IFACE" ]] && { echo "[run-physical] 无法获取默认路由" >&2; exit 1; }

UID_CLIENT=$(id -u "$USER")

echo "[run-physical] default route: via $GW dev $IFACE"
echo "[run-physical] uid: $UID_CLIENT, table: $TABLE"

# 幂等：先清理再添加
ip route flush table "$TABLE" 2>/dev/null || true
ip rule del table "$TABLE" priority "$PRIORITY" 2>/dev/null || true

ip route add default via "$GW" dev "$IFACE" table "$TABLE"
ip rule add uidrange "${UID_CLIENT}-${UID_CLIENT}" table "$TABLE" priority "$PRIORITY"

echo "[run-physical] ready, starting client..."
exec sudo -u "$USER" "$@"
