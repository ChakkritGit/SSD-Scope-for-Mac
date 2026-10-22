#!/bin/bash
# Builds a drag-to-Applications disk image: the app on the left, an Applications link on the right.
set -euo pipefail
PROJECT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$1"
DMG="$2"
PYTHON="$(dirname "$PROJECT")/.toolchain/dmg-venv/bin/python"
if [ ! -x "$PYTHON" ]; then
  /usr/bin/python3 -m venv "$(dirname "$PROJECT")/.toolchain/dmg-venv"
  "$PYTHON" -m pip install --quiet 'dmgbuild==1.6.7' 'Pillow==13.0.0'
fi
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
"$PYTHON" -I "$PROJECT/scripts/dmg-background.py" "$WORK/background.png" 1
"$PYTHON" -I "$PROJECT/scripts/dmg-background.py" "$WORK/background@2x.png" 2
/usr/bin/tiffutil -cathidpicheck "$WORK/background.png" "$WORK/background@2x.png" -out "$WORK/background.tiff" >/dev/null
rm -f "$DMG"
"$PYTHON" -I -m dmgbuild -s "$PROJECT/scripts/dmg-settings.py" \
  -D app="$APP" -D background="$WORK/background.tiff" \
  "SSD Scope for Mac" "$DMG"
printf 'Disk image: %s\n' "$DMG"
