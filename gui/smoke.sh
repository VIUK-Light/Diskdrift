#!/usr/bin/env bash
# Build and run the deterministic core smoke test (no GUI).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LIBDIR="${1:-$ROOT/target/release}"
WORK="$(mktemp -d)"
cleanup() {
  rm -r "$WORK" 2>/dev/null || true
}
trap cleanup EXIT

if [ ! -f "$LIBDIR/libdiskdrift.a" ]; then
  echo "error: $LIBDIR/libdiskdrift.a not found (run: cargo build --release)" >&2
  exit 1
fi

ARCH="$(uname -m)"
xcrun swiftc -swift-version 5 -O -target "${ARCH}-apple-macos13.0" \
  -o "$WORK/diskdrift_smoke" \
  -import-objc-header "$ROOT/gui/include/CDiskDrift.h" \
  -L "$LIBDIR" -ldiskdrift \
  -framework CoreFoundation -framework Security \
  "$ROOT/gui/Sources/Core.swift" "$ROOT/gui/Smoke/main.swift"

"$WORK/diskdrift_smoke"
