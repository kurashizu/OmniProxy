#!/bin/bash

# --- Configuration Section ---
INTERFACE="tun0"

# Check if the script is running with root privileges
if [[ $EUID -ne 0 ]]; then
   echo "This script must be run with sudo"
   exit 1
fi

echo "Configuring TUN device and routing..."

# 1. Bring up the network interface
ip link set $INTERFACE up

# 2. Configure virtual IP addresses
ip addr add 198.18.0.1/16 dev $INTERFACE
ip -6 addr add fd00::1/64 dev $INTERFACE

# Add static default routes with 'proto static' flag
ip route add default dev $INTERFACE metric 1 proto static
ip -6 route add ::/0 dev $INTERFACE metric 1 proto static

echo "Configuration complete!"
echo "IPv4 Route:"
ip route show default
echo "IPv6 Route:"
ip -6 route show default
