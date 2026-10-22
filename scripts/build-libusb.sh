#!/bin/bash
set -euo pipefail
PROJECT="$(cd "$(dirname "$0")/.." && pwd)"
ARCHIVE="$PROJECT/sources/libusb-1.0.30.tar.bz2"
EXPECTED=fea36f34f9156400209595e300840767ab1a385ede1dc7ee893015aea9c6dbaf
ACTUAL="$(/usr/bin/shasum -a 256 "$ARCHIVE" | /usr/bin/awk '{print $1}')"
if [ "$ACTUAL" != "$EXPECTED" ]; then
  echo 'libusb source checksum mismatch' >&2
  exit 1
fi
BUILD_DIR="$(mktemp -d "${TMPDIR:-/tmp}/scope-libusb.XXXXXX")"
trap 'rm -rf "$BUILD_DIR"' EXIT
/usr/bin/tar -xjf "$ARCHIVE" -C "$BUILD_DIR"
cd "$BUILD_DIR/libusb-1.0.30"
export MACOSX_DEPLOYMENT_TARGET=14.0
# A bounded command length also works when the build sandbox denies sysctl.
# The current SDK exposes pipe2, but our deployment target predates its macOS availability.
lt_cv_sys_max_cmd_len=32768 ac_cv_func_pipe2=no ./configure \
  --enable-shared --disable-static --disable-dependency-tracking --prefix="$PROJECT/runtime/libusb"
/usr/bin/make -j4
/usr/bin/make install
cp COPYING "$PROJECT/sources/LIBUSB-COPYING"
python3 - "$PROJECT" <<'PY'
import json,platform,sys
from pathlib import Path
p=Path(sys.argv[1]); target='aarch64-apple-darwin' if platform.machine()=='arm64' else 'x86_64-apple-darwin'
config='[target.'+target+'."usb-1.0"]\n'
config+='rustc-link-search = ['+json.dumps(str(p/'runtime/libusb/lib'))+']\n'
config+='rustc-link-lib = ["dylib=usb-1.0"]\n'
config+='version_number = "1.0.30"\n'
(p/'runtime/libusb-link.toml').write_text(config)
PY
