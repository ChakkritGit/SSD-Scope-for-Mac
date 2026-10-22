#!/bin/bash
set -euo pipefail
PROJECT="$(cd "$(dirname "$0")/.." && pwd)"
OUTPUT="$(dirname "$PROJECT")/dist"
if [ ! -x "$PROJECT/runtime/smartctl" ]; then
  "$PROJECT/scripts/build-smartctl.sh"
fi
"$PROJECT/scripts/build-libusb.sh"
export MACOSX_DEPLOYMENT_TARGET=14.0
APP="$OUTPUT/SSD Scope for Mac.app"
# A stable signing identity keeps macOS privacy grants (removable volumes) across updates;
# an ad-hoc signature changes with every build, so macOS asks again after each one.
if [ -z "${SIGN_IDENTITY:-}" ]; then
  SIGN_IDENTITY="$(/usr/bin/security find-identity -v -p codesigning 2>/dev/null \
    | /usr/bin/awk -F'"' '/Developer ID Application|Apple Development/ { print $2; exit }')"
fi
SIGN_IDENTITY="${SIGN_IDENTITY:--}"
rm -rf "$APP"
"$PROJECT/scripts/cargo.sh" build --release --locked --features experimental-usb --target aarch64-apple-darwin
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$APP/Contents/Frameworks"
cp "$PROJECT/target/aarch64-apple-darwin/release/ssd-scope-mac" "$APP/Contents/MacOS/ssd-scope-mac"
cp "$PROJECT/target/aarch64-apple-darwin/release/scope-cli" "$APP/Contents/MacOS/scope-cli"
cp "$PROJECT/target/aarch64-apple-darwin/release/scope-usb-smart" "$APP/Contents/MacOS/scope-usb-smart"
cp "$PROJECT/runtime/smartctl" "$APP/Contents/MacOS/smartctl"
cp "$PROJECT/runtime/libusb/lib/libusb-1.0.0.dylib" "$APP/Contents/Frameworks/"
/usr/bin/install_name_tool -id '@rpath/libusb-1.0.0.dylib' "$APP/Contents/Frameworks/libusb-1.0.0.dylib"
for BINARY in ssd-scope-mac scope-cli scope-usb-smart; do
  /usr/bin/install_name_tool -change "$PROJECT/runtime/libusb/lib/libusb-1.0.0.dylib" \
    '@loader_path/../Frameworks/libusb-1.0.0.dylib' "$APP/Contents/MacOS/$BINARY"
done
"$PROJECT/target/aarch64-apple-darwin/release/scope-cli" write-icons "$PROJECT/assets/generated"
cp "$PROJECT/assets/generated/Scope.icns" "$APP/Contents/Resources/Scope.icns"
cp "$PROJECT/LICENSE" "$APP/Contents/Resources/LICENSE"
cp "$PROJECT/README.md" "$APP/Contents/Resources/README.md"
cp "$PROJECT/THIRD_PARTY_NOTICES.md" "$APP/Contents/Resources/THIRD_PARTY_NOTICES.md"
cp "$PROJECT/SECURITY.md" "$PROJECT/SOURCE_PROVENANCE.md" "$APP/Contents/Resources/"
mkdir -p "$APP/Contents/Resources/docs"
cp "$PROJECT/docs/"*.md "$APP/Contents/Resources/docs/"
mkdir -p "$APP/Contents/Resources/smartmontools-sources/sources" "$APP/Contents/Resources/smartmontools-sources/scripts"
cp "$PROJECT/sources/smartmontools-7.5.tar.gz" "$PROJECT/sources/SMARTMONTOOLS-COPYING" "$APP/Contents/Resources/smartmontools-sources/sources/"
cp "$PROJECT/scripts/build-smartctl.sh" "$APP/Contents/Resources/smartmontools-sources/scripts/"
cp "$PROJECT/runtime/smartctl-build-info.txt" "$APP/Contents/Resources/smartmontools-sources/"
mkdir -p "$APP/Contents/Resources/libusb-sources/sources" "$APP/Contents/Resources/libusb-sources/scripts"
cp "$PROJECT/sources/libusb-1.0.30.tar.bz2" "$PROJECT/sources/LIBUSB-COPYING" "$APP/Contents/Resources/libusb-sources/sources/"
cp "$PROJECT/scripts/build-libusb.sh" "$APP/Contents/Resources/libusb-sources/scripts/"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>SSD Scope for Mac</string>
<key>CFBundleDisplayName</key><string>SSD Scope for Mac</string>
<key>CFBundleIdentifier</key><string>io.local.ssdscope.rust</string>
<key>CFBundleExecutable</key><string>ssd-scope-mac</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.5.1</string>
<key>CFBundleVersion</key><string>7</string>
<key>CFBundleIconFile</key><string>Scope</string>
<key>LSMinimumSystemVersion</key><string>14.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSSupportsAutomaticGraphicsSwitching</key><true/>
<key>NSPrincipalClass</key><string>NSApplication</string>
<key>NSRemovableVolumesUsageDescription</key><string>SSD Scope shows free space, activity and health for external drives.</string>
</dict></plist>
PLIST
if [ -d "$PROJECT/THIRD_PARTY_LICENSES" ]; then
  mkdir -p "$APP/Contents/Resources/THIRD_PARTY_LICENSES"
  cp -R "$PROJECT/THIRD_PARTY_LICENSES/." "$APP/Contents/Resources/THIRD_PARTY_LICENSES/"
fi
# No hardened runtime: the libusb helper must load the system IOUSBLib plug-in as root,
# and the app is not notarized, so the runtime adds no Gatekeeper benefit here.
sign() { /usr/bin/codesign --force --timestamp=none --sign "$SIGN_IDENTITY" "$@"; }
sign "$APP/Contents/Frameworks/libusb-1.0.0.dylib"
for BINARY in smartctl scope-cli scope-usb-smart; do
  sign "$APP/Contents/MacOS/$BINARY"
done
sign "$APP"
/usr/bin/codesign --verify --deep --strict "$APP"
rm -f "$OUTPUT/SSD-Scope-for-Mac-arm64.zip"
/usr/bin/ditto -c -k --sequesterRsrc --keepParent "$APP" "$OUTPUT/SSD-Scope-for-Mac-arm64.zip"
"$PROJECT/scripts/make-dmg.sh" "$APP" "$OUTPUT/SSD-Scope-for-Mac-0.5.1-arm64.dmg"
printf 'App: %s\nSigned by: %s\nArchive: %s\n' "$APP" "$SIGN_IDENTITY" "$OUTPUT/SSD-Scope-for-Mac-arm64.zip"
