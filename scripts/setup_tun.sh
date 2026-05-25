#!/bin/bash
# Usage: sudo ./setup_tun.sh
#   Sets up tun0 interface with fake IP range (198.18.0.0/16) for proxy routing.
# Prerequisites: root privileges required

set -e

INTERFACE="tun0"

if [[ $EUID -ne 0 ]]; then
   echo "This script must be run with sudo"
   exit 1
fi

echo "Configuring TUN device and routing..."

# Bring up the network interface
ip link set $INTERFACE up

# Configure virtual IP addresses (fake-ip range for mihomo)
ip addr add 198.18.0.1/16 dev $INTERFACE
ip -6 addr add fd00::1/64 dev $INTERFACE

# Add static default routes
ip route add default dev $INTERFACE metric 1 proto static
ip -6 route add ::/0 dev $INTERFACE metric 1 proto static

echo "Configuration complete!"
echo "IPv4 Route:"
ip route show default
echo "IPv6 Route:"
ip -6 route show default