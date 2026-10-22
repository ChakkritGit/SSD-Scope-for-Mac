#!/bin/bash
# Explicit interactive hardware handoff; run only from a staged internal folder.
set -euo pipefail
umask 077
BASE="$(cd "$(dirname "$0")" && pwd)"
cd "$BASE"
if [ "$EUID" -eq 0 ]; then
  echo 'Run this launcher as your regular user. Only the USB helper uses sudo.' >&2
  exit 1
fi
if [ ! -x "$BASE/MacOS/scope-usb-smart" ]; then
  echo 'First use scripts/prepare-usb-test.sh to copy the tools to the internal disk.' >&2
  exit 1
fi
python3 - "$BASE" <<'PY'
import os,stat,sys
from pathlib import Path
p=Path(sys.argv[1]); s=p.lstat()
if not str(p).startswith('/private/tmp/ssd-scope-usb.') or s.st_uid!=os.getuid() or stat.S_IMODE(s.st_mode)!=0o700:
    raise SystemExit('Use an owned private folder created by prepare-usb-test.sh')
PY
for BINARY in MacOS/scope-usb-smart MacOS/scope-cli MacOS/smartctl Frameworks/libusb-1.0.0.dylib; do
  /usr/bin/codesign --verify --strict "$BASE/$BINARY"
done
printf 'Experimental SMART snapshot. Full Security is retained.\n'
printf 'Save and close all SSD files, apps, shells and the Codex session on that drive first.\n'
read -r -p 'Physical disk identifier from Disk Utility (diskN): ' DEVICE
if [[ ! "$DEVICE" =~ ^disk[0-9]{1,8}$ ]]; then
  echo 'Invalid whole-disk identifier.' >&2
  exit 1
fi
"$BASE/MacOS/scope-usb-smart" probe "$DEVICE" > probe-before.json
cat probe-before.json
REGISTRY="$(python3 - "$DEVICE" <<'PY'
import json,sys
p=json.load(open('probe-before.json'))
r=p['usb']['registry_id']
if p['device']!=sys.argv[1] or type(r) is not int or not 0<r<2**64 or not p['bot_available'] or p['warnings']:
    raise SystemExit('Incomplete probe; stop here')
print(r)
PY
)"
printf '\nCheck the model, capacity and mount paths above.\n'
printf 'This will normally unmount that whole disk, temporarily capture its USB device,\n'
printf 'read only IDENTIFY/SMART/SENSE and attempt driver restoration. Reconnection may be needed.\n'
printf 'It will not force unmount, format, erase or write disk data.\n'
read -r -p 'Type READ to authorize this one capture, or anything else to stop: ' ANSWER
if [ "$ANSWER" != READ ]; then
  echo 'Stopped before unmount or USB capture.'
  exit 0
fi
"$BASE/MacOS/scope-usb-smart" probe "$DEVICE" > probe-pre-unmount.json
python3 - <<'PY'
import json
a=json.load(open('probe-before.json')); b=json.load(open('probe-pre-unmount.json'))
if any(a[k]!=b[k] for k in ('device','capacity','usb','mounted_paths')) or b['warnings'] or not b['bot_available']:
    raise SystemExit('Device identity or mount state changed; stop and prepare a new probe')
PY
/usr/sbin/diskutil unmountDisk "$DEVICE"
printf '\nEnter any administrator password in this Terminal only, never in chat.\n'
CAPTURE_STATUS=0
/usr/bin/sudo "$BASE/MacOS/scope-usb-smart" capture-read "$DEVICE" \
  --registry-id "$REGISTRY" --allow-usb-capture > capture.json 2> capture-error.txt || CAPTURE_STATUS=$?
if [ "$CAPTURE_STATUS" -eq 0 ] || [ "$CAPTURE_STATUS" -eq 2 ]; then
  "$BASE/MacOS/scope-cli" decode-usb-report "$BASE/capture.json" > snapshot.json
  cat snapshot.json
else
  cat capture-error.txt >&2
fi
printf '\nPrivate results: %s\n' "$BASE"
printf 'If the SSD does not return in Finder, reconnect it. Disk numbers may change.\n'
printf 'This is a captured snapshot, not live widget data. Keep the reports private.\n'
read -r -p 'Press Return to close: ' _DONE
exit "$CAPTURE_STATUS"
