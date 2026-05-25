#!/bin/bash
TUN2SOCKS="${1:-./tun2socks}"
exec "$TUN2SOCKS" -device tun0 -proxy socks5://127.0.0.1:1080