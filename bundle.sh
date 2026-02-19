#!/bin/bash
set -e

APP_NAME="Songindex"
APP_DIR="target/${APP_NAME}.app"

echo "Building release binary..."
cargo build --release

echo "Creating app bundle..."
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"
mkdir -p "$APP_DIR/Contents/Resources"

cp target/release/songindex "$APP_DIR/Contents/MacOS/songindex"
cp bundle/Info.plist "$APP_DIR/Contents/Info.plist"

# Copy icon if it exists
if [ -f bundle/AppIcon.icns ]; then
    cp bundle/AppIcon.icns "$APP_DIR/Contents/Resources/AppIcon.icns"
fi

echo ""
echo "App bundle created at: $APP_DIR"
echo ""
echo "To install, run:"
echo "  cp -r \"$APP_DIR\" /Applications/"
echo ""
echo "Data stored in: ~/Library/Application Support/songindex/"
