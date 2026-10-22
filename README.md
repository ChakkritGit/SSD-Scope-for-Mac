# SSD Scope for Mac — experimental

A clean, minimal Rust storage dashboard for macOS Apple Silicon, including M4.
An independent prototype, not an official Transcend product or a complete port of SSD Scope.

**The main USB SMART goal is incomplete.** On the tested TSD302 / RTL9201 SSD, this app
still cannot read temperature, SMART health or endurance. UI, activity counters and file-operation
tests do not establish SMART support. See [source and native transport review](docs/NATIVE-TRANSPORT.md).

แอป Rust พร้อมกราฟกิจกรรม SSD จริง และวิดเจตบน menu bar ของ macOS

## Features

- Light minimal interface with original icons, live activity chart and capacity ring.
- Real read/write throughput from IORegistry counters; bounded two-minute history; no simulated telemetry.
- Native top-right menu bar icon and widget: free space, I/O MB/s or temperature.
  Its native macOS menu opens the dashboard, Settings, refreshes and quits.
- Settings to enable/disable the menu bar icon, value beside it, value type and behavior when closing the window.
  Preferences are saved locally. Native macOS app menus include Settings (Cmd+,), refresh (Cmd+R),
  page navigation (Cmd+1–4) and guarded quit (Cmd+Q).
- Physical drive / volume information, APFS container space and available native SMART metrics.
- USB bridge detection and bundled smartctl diagnostics using auto, ATA, SAT-12 and SAT-16 read modes.
- Temporary-file benchmark with read-back verification and owned-file cleanup.
- File backups into a new private directory, SHA-256 verification, macOS metadata preservation through ditto,
  recovery into another new folder, operation review and cancellation.

## USB SMART status

For the tested SSD256GB volume, model TSD302 uses a Realtek RTL9201 bridge (0bda:9201).
IORegistry activity counters work. On the tested macOS 27.2 system, auto/ATA SMART reads returned
Operation not supported by device; SAT modes returned Not a device of type scsi.
The app reports that its current USB SMART transport is unavailable. It does not claim the SSD lacks
SMART or invent temperature/endurance. Version 0.3.0 adds a separate experimental userspace
USB BOT/SAT reader. It is implemented and fixture-tested, but has not yet returned a valid
SMART reading from this SSD. It requires the drive to be unmounted and administrator authorization;
it cannot continuously monitor the SSD while macOS uses it. See [direct USB capture](docs/DIRECT-USB.md).
No driver or security-setting change is made by this app.
The Apple Silicon SAT driver package was verified outside the development sandbox as signed, notarized and accepted by Gatekeeper. It has not been installed; kernel-driver approval may require user action and a restart.
See [hardware diagnosis](docs/USB-SMART.md).
Keeping Full Security is supported: capacity and activity remain available, while unsupported SMART
fields remain unavailable. A Linux VM with USB passthrough is a possible alternative for a separate
SMART capture; it is not integrated or verified on this SSD, and macOS cannot use the SSD concurrently.

Firmware flashing, forced TRIM, secure erase, raw restore, ODIN image import and bootable cloning
are not implemented. File backups are not live snapshots; close apps that modify source files first.
SHA-256 checks contents, not every metadata field.

## Build and use

Requires macOS 14+, Xcode Command Line Tools and current stable Rust. Runtime tested on macOS 27.2 arm64;
older macOS versions have not been exercised.

```sh
./scripts/build-smartctl.sh
./scripts/build-libusb.sh
./scripts/cargo.sh test --release --locked --features experimental-usb --tests
./scripts/package.sh
```

The app and ZIP are written to the parent dist directory. The app is ad-hoc signed for local use,
not Developer ID signed or notarized. Rust/Homebrew are not needed to run the bundle.
The bundle includes the exact corresponding smartctl/libusb sources and build instructions.
scripts/cargo.sh uses a workspace-local Rust toolchain if present; otherwise Cargo in PATH.

## Tests and security

```sh
./scripts/cargo.sh test --release --locked --features experimental-usb --tests
./scripts/cargo.sh clippy --release --locked --features experimental-usb --all-targets -- -D warnings
./scripts/cargo.sh fmt -- --check
```

Test writes use owned temporary directories only. No tests erase, partition, restore or write raw user disks.
The suite covers corruption, cancellation, command/path injection, symlink substitution, containment,
private permissions, cleanup, SMART error interpretation and bounded telemetry.
See [SECURITY.md](SECURITY.md) and [validation](docs/VALIDATION.md).

## License and provenance

Original Rust code and geometric icons: [MIT](LICENSE). Third-party terms remain unchanged.
smartctl is a separate GPL-2.0-or-later executable with unmodified corresponding source and license.
The experimental USB helper dynamically links libusb 1.0.30 under LGPL-2.1-or-later;
its library, exact source, license and replacement instructions are included.
The supplied Transcend System Clone / ODIN source was assessed for compatibility; its GPL code,
Windows assets and installer are not incorporated or redistributed here, and are not relabeled MIT.

See [notices](THIRD_PARTY_NOTICES.md), [provenance](SOURCE_PROVENANCE.md) and
[Transcend disclosure](https://transcend-info.com/Legal/?no=10).
