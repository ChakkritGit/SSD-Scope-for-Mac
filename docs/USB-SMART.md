# USB SMART diagnosis

Observed read-only on macOS 27.2 arm64, 2026-10-21:

| Item | Observed value |
| --- | --- |
| Volume / product | SSD256GB APFS / TSD302 |
| Bridge | Realtek RTL9201, 0bda:9201, revision f200 |
| Interface / link | USB Attached SCSI, protocol 0x62 / 5 Gbit/s |
| SMART-capable driver property | Not exposed |
| IORegistry read/write counters | Available |

smartctl 7.5 was built from the checksummed official source. Actual responses:

| Read mode | Exit | Result |
| --- | --- | --- |
| auto | 2 | Operation not supported by device |
| ata | 2 | Operation not supported by device |
| sat | 1 | Not a device of type 'scsi' |
| sat,12 | 1 | Not a device of type 'scsi' |
| sat,16 | 1 | Not a device of type 'scsi' |

The upstream database maps RTL9201 to SAT. The current macOS smartmontools backend does not
implement generic SCSI transport (`get_scsi_device` returns null). Linux results for the same USB ID
therefore do not establish Mac support.

Sources: [database](https://github.com/smartmontools/smartmontools/blob/main/drivedb/drivedb.h),
[Darwin backend](https://github.com/smartmontools/smartmontools/blob/main/lib/os_darwin.cpp),
[bridge report](https://github.com/smartmontools/smartmontools/issues/270).

A compatible macOS SAT SMART driver may expose the ATA SMART API. The
[publisher's linked package](https://binaryfruit.com/drivedx/usb-drive-support),
SATSMARTDriver-0.10.3.pkg for Apple Silicon, was downloaded for inspection only.
An initial sandboxed signature check was unreliable. Rechecking outside the sandbox confirmed **signed by an Apple-issued developer certificate**, **trusted notarization**, and **accepted by Gatekeeper**.
The kernel extension also passed codesign verification and contains arm64e. The installer is signed by Kirill Luzanov, Team ID 4ZNF85T75D. It has not been installed or bundled. No volume was unmounted and no security protection was changed.

The app detects the bridge, reads real activity counters and reports missing SMART transport explicitly.
It can normalize a successful report if a compatible trusted driver exposes it later. Actual USB
temperature/endurance reading remains unverified on this device; missing values do not establish health.

See the [follow-up native transport review](NATIVE-TRANSPORT.md) for the supplied ZIP's exact
IDENTIFY commands, the Apple SCSI interface probe and successfully read BOT/UAS descriptors.
Direct USB transport is an implementation gap in this app; smartctl failure does not prove that
every possible macOS approach requires a kernel extension.

## Keeping Full Security

The owner chose to retain Full Security. No SAT kernel extension was installed.
Changing from Rust to Swift, running smartctl as root, or changing the interface theme does not
implement the missing USB transport. The [smartctl manual](https://github.com/smartmontools/smartmontools/blob/main/src/smartctl.8.in)
documents the Darwin SCSI limitation and the SAT driver requirement.

Other approaches remain possible, but have not been tested on this particular SSD:

- Connect it to a Windows or Linux machine with supported SAT transport and read SMART there.
- Pass the physical USB device to a Linux VM. [UTM documents USB passthrough](https://docs.getutm.app/guest-support/sharing/usb/)
  without a custom kernel driver, including capture limitations. This is a possible diagnostic
  workaround, not a verified live macOS telemetry source. The drive must be released by macOS;
  a shared folder or virtual disk image does not provide access to the USB bridge's SMART commands.
  Do not disconnect a drive that is hosting an active project or VM.
- Develop a new DriverKit extension. This is separate driver work, requires [Apple-granted entitlements
  and signing](https://developer.apple.com/documentation/driverkit/requesting-entitlements-for-driverkit-development),
  and still needs hardware validation. An unsigned ordinary app cannot provide that path by itself.
