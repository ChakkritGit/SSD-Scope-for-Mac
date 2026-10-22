#!/bin/bash
# Stage tools on the internal disk. This script does not unmount or capture a device.
set -euo pipefail
PROJECT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$(dirname "$PROJECT")/dist/SSD Scope for Mac.app"
if [ ! -x "$APP/Contents/MacOS/scope-usb-smart" ]; then
  echo 'Build the app with scripts/package.sh first.' >&2
  exit 1
fi
umask 077
STAGE="$(mktemp -d /private/tmp/ssd-scope-usb.XXXXXX)"
mkdir "$STAGE/MacOS" "$STAGE/Frameworks"
for BINARY in scope-usb-smart scope-cli smartctl; do
  cp "$APP/Contents/MacOS/$BINARY" "$STAGE/MacOS/"
  /usr/bin/codesign --verify --strict "$STAGE/MacOS/$BINARY"
done
cp "$APP/Contents/Frameworks/libusb-1.0.0.dylib" "$STAGE/Frameworks/"
/usr/bin/codesign --verify --strict "$STAGE/Frameworks/libusb-1.0.0.dylib"
cp "$PROJECT/docs/DIRECT-USB.md" "$STAGE/README.md"
cp "$PROJECT/scripts/run-usb-test.sh" "$STAGE/Run-USB-SMART.command"
chmod 700 "$STAGE/Run-USB-SMART.command"
printf 'Prepared tools only (no device access): %s\n' "$STAGE"
