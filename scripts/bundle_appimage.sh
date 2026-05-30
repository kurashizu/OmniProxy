#!/bin/bash
# Creates an AppImage for the GUI binary.
# Usage: ./scripts/bundle_appimage.sh [target-dir]
#        (defaults to ./target/release)

set -euo pipefail

TARGET_DIR="${1:-target/release}"
BINARY="$TARGET_DIR/gui"
APP_DIR="$TARGET_DIR/OmniProxy.AppDir"

if [ ! -f "$BINARY" ]; then
    echo "Binary not found at $BINARY. Build first: cargo build -p gui --release"
    exit 1
fi

rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/usr/bin"
cp "$BINARY" "$APP_DIR/usr/bin/omniproxy-gui"
ln -sf usr/bin/omniproxy-gui "$APP_DIR/AppRun"

# ── .desktop file ───────────────────────────────────────────────────
cat > "$APP_DIR/omniproxy-gui.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=OmniProxy Dashboard
Comment=OmniProxy Proxy & Relay Dashboard
Exec=omniproxy-gui
Icon=omniproxy-gui
Categories=Network;Utility;
Terminal=false
EOF

# ── 1x1 transparent PNG icon (placeholder) ─────────────────────────
printf '\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89\x00\x00\x00\x0bIDATx\x9cc\xf8\x0f\x00\x00\x01\x01\x00\x05\x18\xd8N\x00\x00\x00\x00IEND\xae\x42\x60\x82' \
    > "$APP_DIR/omniproxy-gui.png"

# ── Download & extract appimagetool (no FUSE needed) ───────────────
TOOL_DIR="$TARGET_DIR/appimagetool-extracted"
if [ ! -f "$TOOL_DIR/AppRun" ]; then
    echo "Downloading appimagetool..."
    curl -sSL "https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage" \
        -o "$TARGET_DIR/appimagetool.AppImage"
    chmod +x "$TARGET_DIR/appimagetool.AppImage"
    rm -rf "$TOOL_DIR"
    "$TARGET_DIR/appimagetool.AppImage" --appimage-extract
    mv squashfs-root "$TOOL_DIR"
fi

# ── Package ─────────────────────────────────────────────────────────
OUTPUT="$TARGET_DIR/OmniProxy-Dashboard-x86_64.AppImage"
ARCH=x86_64 "$TOOL_DIR/AppRun" "$APP_DIR" "$OUTPUT"

echo "Created $OUTPUT"
ls -lh "$OUTPUT"
