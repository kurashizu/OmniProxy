#!/bin/bash
# macOS Gatekeeper bypass for OmniProxy binaries
# Run once after extracting the release zip.
# Usage: chmod +x setup_macos.sh && ./setup_macos.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BINARIES=("client" "server" "proxy" "tun2socks")

echo "=== macOS Security Setup for OmniProxy ==="
echo ""

# 1. Remove quarantine attribute
echo "[1/3] Removing quarantine attributes..."
for bin in "${BINARIES[@]}"; do
    if [ -f "$SCRIPT_DIR/$bin" ]; then
        xattr -dr com.apple.quarantine "$SCRIPT_DIR/$bin" 2>/dev/null || true
        echo "  ✓ $bin"
    fi
done

# 2. Ad-hoc code signing
echo "[2/3] Ad-hoc code signing..."
for bin in "${BINARIES[@]}"; do
    if [ -f "$SCRIPT_DIR/$bin" ]; then
        codesign --force --sign - "$SCRIPT_DIR/$bin" 2>/dev/null || true
        echo "  ✓ $bin"
    fi
done

# 3. Ensure executable permissions
echo "[3/3] Setting executable permissions..."
for bin in "${BINARIES[@]}"; do
    if [ -f "$SCRIPT_DIR/$bin" ]; then
        chmod +x "$SCRIPT_DIR/$bin"
        echo "  ✓ $bin"
    fi
done

echo ""
echo "=== Setup complete ==="
echo "Run the proxy with: sudo ./proxy --config ./config.yml"
