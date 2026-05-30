#!/bin/bash
# macOS post-extract security setup for OmniProxy.
# Removes quarantine, ad-hoc signs, and sets executable permissions.
# Usage: ./setup_macos.sh

set -euo pipefail

echo "=== macOS Security Setup for OmniProxy ==="
echo

# ── 1. Remove quarantine ──────────────────────────────────────────
echo "[1/4] Removing quarantine attributes..."
for f in client server proxy; do
    if [ -f "$f" ]; then
        xattr -dr com.apple.quarantine "$f" 2>/dev/null || true
        echo "  ✓ $f"
    fi
done
if [ -d "OmniProxy Dashboard.app" ]; then
    xattr -dr com.apple.quarantine "OmniProxy Dashboard.app" 2>/dev/null || true
    echo "  ✓ OmniProxy Dashboard.app"
fi

# ── 2. Ad-hoc code signing ────────────────────────────────────────
echo "[2/4] Ad-hoc code signing..."
for f in client server proxy; do
    if [ -f "$f" ]; then
        codesign --force --sign - "$f" 2>/dev/null || true
        echo "  ✓ $f"
    fi
done
if [ -d "OmniProxy Dashboard.app" ]; then
    codesign --force --deep --sign - "OmniProxy Dashboard.app" 2>/dev/null || true
    echo "  ✓ OmniProxy Dashboard.app"
fi

# ── 3. Executable permissions ─────────────────────────────────────
echo "[3/4] Setting executable permissions..."
for f in client server proxy; do
    if [ -f "$f" ]; then
        chmod +x "$f"
        echo "  ✓ $f"
    fi
done
if [ -d "OmniProxy Dashboard.app" ]; then
    chmod +x "OmniProxy Dashboard.app/Contents/MacOS/OmniProxy" 2>/dev/null || true
    for b in client proxy; do
        [ -f "OmniProxy Dashboard.app/Contents/MacOS/$b" ] && chmod +x "OmniProxy Dashboard.app/Contents/MacOS/$b"
    done
    echo "  ✓ OmniProxy Dashboard.app"
fi

# ── 4. Copy config if missing ─────────────────────────────────────
echo "[4/4] Config check..."
if [ ! -f config.yml ]; then
    echo "  ⚠ config.yml not found — create one before running"
else
    echo "  ✓ config.yml exists"
fi

echo
echo "=== Setup complete ==="
echo "CLI:     sudo ./proxy --config ./config.yml"
echo "GUI:     open \"OmniProxy Dashboard.app\""
echo "GUI CLI: ./OmniProxy\\ Dashboard.app/Contents/MacOS/OmniProxy"
