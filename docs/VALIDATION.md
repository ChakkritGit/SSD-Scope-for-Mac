# Validation

Local validation: 2026-10-21–22, macOS 27.2 arm64, Rust 1.99.1, Xcode tools.
The minimum supported build target is macOS 14; runtime testing used this machine only.

Version 0.3.0 adds an experimental separate USB capture helper and offline decoder.
Additional read-only IOKit diagnostics
are documented in [NATIVE-TRANSPORT.md](NATIVE-TRANSPORT.md). They read descriptors and query
interface availability, not SMART data; the primary USB SMART acceptance criterion remains unmet.

## Automated checks

- Release tests with experimental-usb: 42 passed, 0 failed
  (8 core behavior, 16 security regressions, 18 USB protocol/report/mount-guard tests).
- Clippy, all targets, warnings denied: passed.
- Formatting: passed.
- arm64 release build and ad-hoc signature verification: passed.
- RustSec audit: no known vulnerability advisories found in the locked dependency graph.
  Database commit `4b587f00996ebe5c05a73eaef105ef241e876414`, 1,348 advisories.

Audit informational warnings remain for unmaintained upstream packages:
`paste 1.0.15` (RUSTSEC-2024-0436), `ttf-parser 0.25.1` (RUSTSEC-2026-0192),
`zbus_names 4.3.5` (RUSTSEC-2026-0371), `zvariant 5.15.1` (RUSTSEC-2026-0370),
and `zvariant_derive 5.15.1` (RUSTSEC-2026-0372). The D-Bus packages are non-macOS dependencies.
The Metal dependency also reports an upstream future-Rust compatibility warning for `block 0.1.6`.
These findings are preserved; an audit is not proof of absence of vulnerabilities.

Tests create owned temporary directories and synthetic fixtures. Coverage includes:
temporary-file benchmark read/write verification and cleanup; backup integrity and same-size corruption;
exclusive destinations and private 0700 permissions; symlink substitution and containment;
literal subprocess arguments, cancellation and timeout; bounded telemetry and counter resets;
SMART failure interpretation without fabricated health/endurance; settings persistence, 0600 files,
invalid/oversized schema rejection and no-follow reads.

USB tests exercise fixed read command encodings; bounded BOT tag/residue/status validation;
stall/timeout/short-write/cancellation paths through mocked bulk I/O; ATA checksum/redaction;
physical/APFS/snapshot mount guards; and private bounded report/stdin handling. The actual bundled
smartctl offline parser decoded a synthetic 31 C fixture, with Passed/Failing/Unknown status cases.
That temperature is test data, not a reading from the user's SSD. No USB capture was performed.

No test writes raw user disks. No erase, partition, firmware flash, kernel-driver installation,
or backup/restore of actual user files was performed during validation.

## Read-only hardware observations

The app detects the SSD256GB APFS volume, TSD302, 256,060,514,304 bytes, Realtek RTL9201
USB bridge 0bda:9201, UAS, 5 Gbit/s. Native IORegistry counters provide real read/write activity.
The bundled smartctl 7.5 probes failed through auto/ATA/SAT modes as recorded in
[USB-SMART.md](USB-SMART.md). Temperature and endurance remain unavailable on this device.
Full Security was retained; the reviewed SAT driver was not installed.

The version 0.3.0 `probe disk6` ran as the regular user outside the development sandbox. It matched
the physical SSD to its IORegistry ID, libusb bus/address and BOT descriptor using libusb 1.0.30,
reported the real mounted path and no inventory warnings. It did not open or capture the USB device
or issue SMART commands. Actual capture, driver restoration and live SMART decoding remain untested.

During GUI validation a later whole-disk `diskutil info` call timed out for disk6, also reproducible
from the CLI. A read-only process sample showed it blocked in the system's
`wholeDiskSupportsLowLevelFormat` / `open` path; volume metadata remained readable. USB physical
inventory and capture selection now use cached public Disk Arbitration descriptions instead of
that whole-disk info call. This avoids the unrelated format-capability query and does not open the
raw device. The revised probe again found the correct SSD, mount path and no discovery warnings.
The repackaged GUI then showed SSD256GB, capacity/free space and its APFS mount immediately,
with temperature Unavailable and health Unknown. The Cmd+, widget Settings controls were enabled.

## UI and distribution

The packaged app launches with Metal. Settings controls and the native macOS File/View/Window/Help
menus and the Cmd+, Settings shortcut were inspected through accessibility. Navigation icons/text now share a single full-row
hit target and hover background. The status item now uses an attached AppKit NSMenu for native
dismissal, instead of an overlay click handler. Automated UI verification of clicking outside or
clicking the status item again was interrupted by computer-use state changes; those interactions
still need manual confirmation on this machine.

The app bundles separate smartctl/USB executables, a replaceable libusb dylib, exact corresponding
smartmontools/libusb sources and license notices. The Cargo graph has complete indexed notices.
The local bundle is ad-hoc signed; it is not Developer ID signed or notarized.
GitHub CI is provided separately; local results do not imply a successful hosted CI run.
