#!/bin/bash
set -euo pipefail
PROJECT="$(cd "$(dirname "$0")/.." && pwd)"
ARCHIVE="$PROJECT/sources/smartmontools-7.5.tar.gz"
EXPECTED=690b83ca331378da9ea0d9d61008c4b22dde391387b9bbad7f29387f2595f76e
ACTUAL="$(/usr/bin/shasum -a 256 "$ARCHIVE" | /usr/bin/awk '{print $1}')"
if [ "$ACTUAL" != "$EXPECTED" ]; then
  echo 'smartmontools source checksum mismatch' >&2
  exit 1
fi
BUILD_DIR="$(mktemp -d "${TMPDIR:-/tmp}/scope-smartctl.XXXXXX")"
trap 'rm -rf "$BUILD_DIR"' EXIT
/usr/bin/tar -xzf "$ARCHIVE" -C "$BUILD_DIR"
cd "$BUILD_DIR/smartmontools-7.5"
export MACOSX_DEPLOYMENT_TARGET=14.0
./configure --prefix=/usr/local --with-nvme-devicescan
/usr/bin/make -j4 smartctl
mkdir -p "$PROJECT/runtime"
cp smartctl "$PROJECT/runtime/smartctl"
/usr/bin/strip "$PROJECT/runtime/smartctl"
cp COPYING "$PROJECT/sources/SMARTMONTOOLS-COPYING"
"$PROJECT/runtime/smartctl" --version > "$PROJECT/runtime/smartctl-build-info.txt"
