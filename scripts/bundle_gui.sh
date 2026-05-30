#!/bin/bash
# Creates a macOS .app bundle for the GUI binary with icon.
# Usage: ./scripts/bundle_gui.sh [target-dir] [icon-path]
#        (defaults to ./target/release and gui/icon.png)

set -euo pipefail

TARGET_DIR="${1:-target/release}"
ICON_PATH="${2:-gui/icon.png}"
APP_NAME="OmniProxy Dashboard"
BINARY="$TARGET_DIR/OmniProxy"
APP_DIR="$TARGET_DIR/${APP_NAME}.app"
CONTENTS="$APP_DIR/Contents"
MACOS="$CONTENTS/MacOS"
RESOURCES="$CONTENTS/Resources"

if [ ! -f "$BINARY" ]; then
    echo "Binary not found at $BINARY. Build first: cargo build -p gui --release"
    exit 1
fi

mkdir -p "$MACOS" "$RESOURCES"

# Copy GUI binary
cp "$BINARY" "$MACOS/OmniProxy"
chmod +x "$MACOS/OmniProxy"

# Copy client/proxy into bundle so relative paths work from inside .app
for bin in client proxy; do
    if [ -f "$TARGET_DIR/$bin" ]; then
        cp "$TARGET_DIR/$bin" "$MACOS/$bin"
        chmod +x "$MACOS/$bin"
    fi
done

# ── Convert PNG → ICNS ────────────────────────────────────────────
if [ -f "$ICON_PATH" ]; then
    ICONSET="$TARGET_DIR/icon.iconset"
    mkdir -p "$ICONSET"
    sips -z 1024 1024 "$ICON_PATH" --out "$ICONSET/icon_512x512@2x.png" >/dev/null
    sips -z 512  512  "$ICON_PATH" --out "$ICONSET/icon_512x512.png"   >/dev/null
    sips -z 512  512  "$ICON_PATH" --out "$ICONSET/icon_256x256@2x.png" >/dev/null
    sips -z 256  256  "$ICON_PATH" --out "$ICONSET/icon_256x256.png"   >/dev/null
    sips -z 256  256  "$ICON_PATH" --out "$ICONSET/icon_128x128@2x.png" >/dev/null
    sips -z 128  128  "$ICON_PATH" --out "$ICONSET/icon_128x128.png"   >/dev/null
    sips -z 64   64   "$ICON_PATH" --out "$ICONSET/icon_32x32@2x.png"  >/dev/null
    sips -z 32   32   "$ICON_PATH" --out "$ICONSET/icon_32x32.png"     >/dev/null
    sips -z 32   32   "$ICON_PATH" --out "$ICONSET/icon_16x16@2x.png"  >/dev/null
    sips -z 16   16   "$ICON_PATH" --out "$ICONSET/icon_16x16.png"     >/dev/null
    iconutil -c icns "$ICONSET" -o "$RESOURCES/icon.icns"
    rm -rf "$ICONSET"
    ICON_FILE="<key>CFBundleIconFile</key>
    <string>icon</string>"
else
    echo "Warning: icon not found at $ICON_PATH, skipping icon"
    ICON_FILE=""
fi

cat > "$CONTENTS/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>OmniProxy</string>
    <key>CFBundleIdentifier</key>
    <string>com.omniproxy.gui</string>
    <key>CFBundleName</key>
    <string>OmniProxy Dashboard</string>
    <key>CFBundleVersion</key>
    <string>1.0</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSUIElement</key>
    <true/>
    $ICON_FILE
</dict>
</plist>
EOF

echo "Created $APP_DIR"
