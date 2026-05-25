#!/bin/bash
# Usage: ./run_tun2socks.sh [TUN2SOCKS_PATH]
#   TUN2SOCKS_PATH  Path to tun2socks binary (default: ./tun2socks)
# Requires: tun2socks, client running on 127.0.0.1:1080

TUN2SOCKS="${1:-./tun2socks}"
exec "$TUN2SOCKS" -device tun0 -proxy socks5://127.0.0.1:1080 -loglevel error