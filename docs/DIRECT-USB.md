# Experimental direct USB SMART capture

Version 0.4.0 provides a separate Rust diagnostic and GUI import for the reviewed TSD302 /
RTL9201 USB bridge (0bda:9201). A user-authorized SAT-16 capture on 2026-10-22 returned valid
SMART data from TS256GMTS830S: 52 C, 9,503 hours and 5,775 power cycles. The helper reported
successful driver restoration. SMART RETURN STATUS sense was unavailable; overall status is
Unknown. Validation is limited to this device and one capture; continuous USB monitoring is unavailable.

Full Security remains enabled. No kernel extension, daemon, startup item or security-policy
change is installed. The dashboard does not request root or capture USB during polling.

## Why this is separate

The macOS smartctl/SCSI path tested here does not expose SAT access. libusb's macOS capture
path can temporarily detach the kernel driver for the entire USB device, requiring administrator
authorization or an appropriate entitlement. This interrupts macOS access and can re-enumerate
the device. The SSD must be unmounted first. The helper never unmounts or forces access itself.

The current workspace is on that SSD. Save and close its files and run any capture from an owned
folder on the internal disk, after explicitly agreeing to this interruption. Do not capture the
SSD while building, backing up, using its files, or running binaries from it.

## Implemented boundaries

- Only whole external USB disks, the reviewed VID/PID and cached configuration 1/interface 0/
  alternate 0/BOT endpoints 81 IN and 02 OUT are accepted.
- Device selection combines IORegistry ID, USB location/bus and device address. A fresh identity
  and mount check immediately precedes capture. A stale registry ID is refused.
- Physical partitions and APFS synthesized disks/snapshots are checked against kernel mounts,
  independently of volume inventory. Incomplete discovery fails closed.
- Fixed commands only: ATA IDENTIFY DEVICE (EC), SMART READ DATA (B0/D0), READ THRESHOLDS
  (B0/D1), RETURN STATUS (B0/DA), and SCSI REQUEST SENSE (03). SAT-16 then SAT-12 are tried.
  Bulk OUT contains only a 31-byte BOT command wrapper. There is no payload data-out, arbitrary
  CDB interface, SMART enable/write/test, firmware update, erase or raw block access.
- Sector checksums, attribute structure, BOT signature/tag/residue/status and lengths are checked.
  Protocol errors abort. Only bounded endpoint-stall handling is attempted; no reset/retry loop.
- USB transfers have five-second timeouts and a two-minute command-session deadline. SIGINT and
  SIGTERM request cooperative cancellation. Driver restoration is attempted on success and errors.
  SIGKILL, power loss or backend failure can bypass/prevent cleanup. Reconnection may be required.
  Opening/capture/driver restoration includes native library calls whose total duration is not
  guaranteed by the bulk-transfer deadline.

These safeguards reduce risk; this is experimental hardware access, not a safety certification.
There is a mount-state race between the final check and USB capture. Do not mount or access the
SSD concurrently. A successful driver-reattach call does not prove that the volume has remounted.

## Build and read-only probe

```sh
./scripts/build-smartctl.sh
./scripts/build-libusb.sh
./scripts/cargo.sh build --release --locked --features experimental-usb
./target/release/scope-usb-smart probe diskN
```

Probe reads inventory and cached descriptors only. It does not open/claim/capture the USB device,
send SMART, unmount or write disk data. The returned mount paths must be empty before capture.
The official bundle dynamically uses libusb 1.0.30 rather than the older C library in libusb1-sys.

## Capture procedure, only after agreeing to interruption

1. Save/close all files, apps, shells and working directories using the target SSD. Prepare the
   helper and its `Frameworks` library in an owned private folder on the internal disk. Preserve
   their `MacOS` / `Frameworks` relative layout. The dashboard does not perform these steps.
   `scripts/prepare-usb-test.sh` prepares such a folder under `/private/tmp` and verifies signatures;
   it does not touch the device. Its contents are temporary and may be removed by macOS cleanup.
   Its `Run-USB-SMART.command` launcher probes again, asks you to verify the disk and type READ
   before a normal unmount and one administrator-authorized capture. You can use that launcher
   instead of the manual commands below. If a Codex session has its working directory on the
   SSD, end that session first; changing only a child shell's directory does not release it.
2. Run `probe diskN` again and record its current registry ID. Verify the model, capacity and
   mounted volume are the intended SSD. Disk identifiers may change after reconnecting.
3. Unmount the whole disk normally with Disk Utility or `diskutil unmountDisk diskN`.
   Do not force an unmount. If it fails, stop and close remaining users of the SSD.
4. From the internal folder, run the following with the current identity. Enter any administrator
   password only in macOS/Terminal, never in chat. Use an owner-only report path (umask 077).

```sh
sudo ./MacOS/scope-usb-smart capture-read diskN --registry-id NUMBER --allow-usb-capture > capture.json
```

5. Keep stdout/report and stderr/error private. The helper attempts to release the interface and
   restore the kernel driver. If that fails, reconnect the SSD. Identify it afresh before manually
   remounting; do not assume its old disk number is still correct.
6. After access is restored, decode as your regular user with the matching packaged tools:

```sh
./MacOS/scope-cli decode-usb-report capture.json
```

The report redacts ATA serial and WWN fields but still contains private health/identity data.
Do not commit it to GitHub. Decoding uses smartctl's offline replay parser, not another live read.
Overall health is accepted only from valid current ATA return registers. Missing status remains
Unknown even if temperature attributes decode. Values are a timestamped snapshot.

## Import into the app

Mount the SSD again, open the app, select its data volume/drive and choose S.M.A.R.T. →
Import USB snapshot. Pick `capture.json` from a read of that SSD (not the decoded snapshot.json).
The importer checks ATA capacity and the reviewed bridge, then binds the explicitly selected
mounted volume UUID. The same UUID/capacity/bridge must match on subsequent launches; disk numbers
may change. A copied JSON file is not a cryptographically authenticated measurement, so import only
a capture you know belongs to that SSD. Remove saved snapshot clears that drive's cached reading.

Overview and SMART show the temperature, capture time/age and `not live` label. The temperature
widget appends `snapshot`; its menu shows the capture date. I/O activity remains live. Raw and
normalized ATA attributes are separate columns. An attribute's raw value named Remaining_Lifetime
is not automatically interpreted as a percent; the app does not manufacture a health/endurance score.
The count at/below nonzero attribute thresholds does not replace SMART RETURN STATUS.

CLI import, as the regular user with the intended SSD mounted:

```sh
./MacOS/scope-cli import-usb-report /path/capture.json diskN
```

Captures are stored privately in `~/Library/Application Support/SSD Scope/usb-captures.json`
(0600 file, 0700 owner directory), with bounded no-follow reads and atomic saves. They are never
uploaded. Importing/reloading a capture performs offline decoding and cached discovery, not USB
capture, unmounting or administrator authorization.

## Tests

The experimental feature tests fixed packet encodings, malformed/short/stalled BOT transactions,
timeouts/cancellation, checksums/redaction, mounted/stale-device rejection, APFS snapshot mapping,
bounded no-follow report loading and offline parsing with real bundled smartctl. All device
responses in those tests are synthetic. Test writes are confined to owned temporary files.

## Replacing libusb

The bundle links a separate LGPL library at `Contents/Frameworks/libusb-1.0.0.dylib`.
Its unmodified source and build script are in `Contents/Resources/libusb-sources`.
Copy that directory to a writable location and run `scripts/build-libusb.sh`; Xcode Command Line
Tools and Python 3 are required. You may modify the library and replace the bundled dylib with
an ABI-compatible build. Capture currently requires reported libusb version 1.0.30 or newer.

After replacing the library in a copy of the app, update its install name and ad-hoc signatures:

```sh
install_name_tool -id '@rpath/libusb-1.0.0.dylib' '/path/App.app/Contents/Frameworks/libusb-1.0.0.dylib'
codesign --force --sign - '/path/App.app/Contents/Frameworks/libusb-1.0.0.dylib'
codesign --force --sign - '/path/App.app'
```

No restriction on reverse engineering for debugging library modifications is imposed. Preserve
LGPL notices/source when redistributing; original MIT Rust code is available in this repository.
The local app has no Developer ID notarization or hardened-runtime library restriction.
