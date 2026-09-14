#!/usr/bin/env bash
# Build DiskDrift.app (SwiftUI frontend) linked against the Rust core.
#
# Usage: gui/build.sh [--arch arm64|x86_64] [--lib <dir>] [--out <app>] [--version <v>]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ARCH="$(uname -m)"
LIBDIR="$ROOT/target/release"
OUT="$ROOT/dist/DiskDrift.app"
VERSION=""

usage() {
  sed -n '2,4p' "$0"
}

while [ $# -gt 0 ]; do
  case "$1" in
    --arch) ARCH="$2"; shift 2 ;;
    --lib) LIBDIR="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 1 ;;
  esac
done

if [ -z "$VERSION" ]; then
  VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
fi

if [ ! -f "$LIBDIR/libdiskdrift.a" ]; then
  echo "Building Rust static library..."
  (cd "$ROOT" && cargo build --release)
fi

APP="$OUT"
rm -r "$APP" 2>/dev/null || true
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleExecutable</key><string>DiskDrift</string>
  <key>CFBundleIdentifier</key><string>com.viuk.diskdrift</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>DiskDrift</string>
  <key>CFBundleDisplayName</key><string>DiskDrift</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
  <key>NSHumanReadableCopyright</key><string>MIT licensed. DiskDrift contributors.</string>
</dict>
</plist>
PLIST

xcrun swiftc -swift-version 5 -O -parse-as-library \
  -target "${ARCH}-apple-macos13.0" \
  -o "$APP/Contents/MacOS/DiskDrift" \
  -import-objc-header "$ROOT/gui/include/CDiskDrift.h" \
  -L "$LIBDIR" -ldiskdrift \
  -framework SwiftUI -framework AppKit -framework Foundation \
  -framework CoreFoundation -framework Security \
  "$ROOT"/gui/Sources/*.swift

# Ad-hoc signature so the app launches locally without a developer account.
codesign --force --sign - "$APP" >/dev/null 2>&1 || true

echo "Built $APP (DiskDrift $VERSION, $ARCH)"
