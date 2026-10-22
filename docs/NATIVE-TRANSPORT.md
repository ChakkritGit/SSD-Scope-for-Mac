# SMART implementation status and source review

The actual SSD now returns temperature and SMART attributes through a separate user-authorized
USB capture on Apple Silicon macOS with Full Security. Overall SMART health and verified endurance
remain unavailable. This is a snapshot path; mounted continuous USB SMART monitoring is not delivered.

## What the supplied ZIP contains

Archive SHA-256: `6244107095b69f2b0db401f55de34baf00b2098d7e15f102196aeaf57faec80c`.
The archive is System Clone / ODIN source. In `src/ODIN/ODINDlg.cpp`:

- `IsTSModelName_ATA`, lines 698–727: data-in ATA pass-through via Windows
  `DeviceIoControl(IOCTL_ATA_PASS_THROUGH_DIRECT)`, command register `0xEC`.
- `IsTSModelName_SCSI`, lines 775–803: ATA pass-through CDB `0xA1`, embedded command `0xEC`,
  via Windows `IOCTL_SCSI_PASS_THROUGH_DIRECT`.
- Both functions inspect IDENTIFY DEVICE model-name words and the `TS` prefix.

This search found model-identification pass-through, not an implementation of SMART temperature,
SMART READ DATA or health/endurance interpretation. The ZIP does not provide the complete
SSD Scope SMART suite. Windows pass-through calls cannot be reused as macOS calls.
The ZIP and its GPL code have not been incorporated or relabeled MIT.

## What has actually been checked on macOS

Read-only follow-up on 2026-10-22, Full Security retained:

| Path | Observed result | What it establishes |
| --- | --- | --- |
| Bundled smartctl auto / ATA / SAT | No valid report; see USB-SMART.md | The app's current helper path fails |
| Apple SCSI device plug-in on the target's `IOSCSIPeripheralDeviceType00` ancestor | `0xe00002c7`, `kIOReturnUnsupported` | That driver did not expose this requested SCSI user-client interface |
| Cached USB configuration through IOKit USB device interface 650 | Success outside the development sandbox | The device's cached USB descriptors can be read |
| USB configuration 1, interface 0, alternate 0 | class 08, subclass 06, protocol 50, endpoints 81/02 | The descriptor advertises Bulk-Only Transport (BOT) |
| Same interface, alternate 1 | protocol 62, endpoints 81/02/83/04 | The descriptor also advertises UAS |

The initial sandboxed USB plug-in query returned `0xe00002be`; it succeeded when rerun outside
that sandbox as the same non-root user. Sandbox failures must not be presented as proof of a
macOS or hardware limitation.

The original diagnostics in `diagnostics/` use C only to call typed Apple SDK interfaces; the app
remains Rust. They do not open or seize the USB device, claim interfaces, obtain exclusive SCSI
access, send SCSI commands, reset devices, unmount volumes or write disk data. They print no serials.

## Experimental transport in version 0.3.0

The separate `scope-usb-smart` helper now implements fixed BOT/SAT data-in commands with
rusb and libusb 1.0.30. Read-only `probe` matched the actual registry ID, USB address and cached
BOT descriptor; both disk inventory and the kernel mount check found `/Volumes/SSD256GB`.
Capture refuses that mounted state. Protocol, mount guards and offline decoding are fixture-tested.
On 2026-10-22 the owner explicitly authorized one capture after unmounting. SAT-16 returned
IDENTIFY/SMART data and thresholds from TS256GMTS830S. Temperature was 52 C, power-on hours 9,503,
and cycles 5,775; driver restoration was reported successful. The private capture validated
sector checksums and decoded offline. No private capture file or serial is committed to this repository.
SMART RETURN STATUS sense was unavailable; overall health remains Unknown.

This remains a limited experimental hardware path. The
[libusb macOS backend](https://github.com/libusb/libusb/blob/master/libusb/os/darwin_usb.c)
and [capture implementation discussion](https://github.com/libusb/libusb/pull/911)
document whole-device capture with root privilege or the appropriate entitlement and user
authorization. This is a distinct approach from installing a SAT kernel extension. No security-policy
change was made. The current project is on the target SSD,
so capturing it during active work would interrupt that workspace.

See [DIRECT-USB.md](DIRECT-USB.md) for the implemented allowlist, recovery limits and staged
procedure. A one-time capture is not continuous telemetry while macOS mounts the drive.

Earlier wording that a SAT driver was the only possible next step was too categorical.
Version 0.2.1 reports `transport_unavailable` and identifies the implementation gap explicitly.
