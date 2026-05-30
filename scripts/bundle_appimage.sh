#!/bin/bash
# Creates an AppImage for the GUI binary.
# Usage: ./scripts/bundle_appimage.sh [target-dir] [icon-path]
#        (defaults to ./target/release and gui/icon.png)

set -euo pipefail

TARGET_DIR="${1:-target/release}"
ICON_PATH="${2:-gui/icon.png}"
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

# ── Icon ────────────────────────────────────────────────────────────
if [ -f "$ICON_PATH" ]; then
    # AppImage expects 256x256 PNG
    if command -v sips &>/dev/null; then
        sips -z 256 256 "$ICON_PATH" --out "$APP_DIR/omniproxy-gui.png" >/dev/null
    elif command -v convert &>/dev/null; then
        convert "$ICON_PATH" -resize 256x256 "$APP_DIR/omniproxy-gui.png"
    else
        cp "$ICON_PATH" "$APP_DIR/omniproxy-gui.png"
    fi
else
    echo "Warning: icon not found at $ICON_PATH, using placeholder"
    printf '\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89\x00\x00\x00\x0bIDATx\x9cc\xf8\x0f\x00\x00\x01\x01\x00\x05\x18\xd8N\x00\x00\x00\x00IEND\xae\x42\x60\x82' \
        > "$APP_DIR/omniproxy-gui.png"
fi

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
ARCH=$(uname -m)
OUTPUT="$TARGET_DIR/OmniProxy-Dashboard-${ARCH}.AppImage"
ARCH=$ARCH "$TOOL_DIR/AppRun" "$APP_DIR" "$OUTPUT"

echo "Created $OUTPUT"
ls -lh "$OUTPUT"
