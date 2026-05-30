#!/bin/bash
# Creates a minimal macOS .app bundle for the GUI binary.
# Usage: ./scripts/bundle_gui.sh [target-dir]
#        (defaults to ./target/release)

set -euo pipefail

TARGET_DIR="${1:-target/release}"
APP_NAME="OmniProxy Dashboard"
BINARY="$TARGET_DIR/gui"
APP_DIR="$TARGET_DIR/${APP_NAME}.app"
CONTENTS="$APP_DIR/Contents"
MACOS="$CONTENTS/MacOS"
RESOURCES="$CONTENTS/Resources"

if [ ! -f "$BINARY" ]; then
    echo "Binary not found at $BINARY. Build first: cargo build -p gui --release"
    exit 1
fi

mkdir -p "$MACOS" "$RESOURCES"
cp "$BINARY" "$MACOS/gui"

cat > "$CONTENTS/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>gui</string>
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
</dict>
</plist>
EOF

echo "Created $APP_DIR"
