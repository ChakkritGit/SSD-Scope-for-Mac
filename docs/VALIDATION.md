# Validation

Local validation: 2026-10-21–22, macOS 27.2 arm64, Rust 1.99.1, Xcode tools.
The minimum supported build target is macOS 14; runtime testing used this machine only.

## Automated checks

- Release tests: 24 passed, 0 failed (8 core behavior, 16 security regressions).
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

No test writes raw user disks. No erase, partition, firmware flash, kernel-driver installation,
or backup/restore of actual user files was performed during validation.

## Read-only hardware observations

The app detects the SSD256GB APFS volume, TSD302, 256,060,514,304 bytes, Realtek RTL9201
USB bridge 0bda:9201, UAS, 5 Gbit/s. Native IORegistry counters provide real read/write activity.
The bundled smartctl 7.5 probes failed through auto/ATA/SAT modes as recorded in
[USB-SMART.md](USB-SMART.md). Temperature and endurance remain unavailable on this device.
Full Security was retained; the reviewed SAT driver was not installed.

## UI and distribution

The packaged app launches with Metal. Settings controls and the native macOS File/View/Window/Help
menus and the Cmd+, Settings shortcut were inspected through accessibility. Navigation icons/text now share a single full-row
hit target and hover background. The status item now uses an attached AppKit NSMenu for native
dismissal, instead of an overlay click handler. Automated UI verification of clicking outside or
clicking the status item again was interrupted by computer-use state changes; those interactions
still need manual confirmation on this machine.

The app bundles a separate smartctl executable, exact corresponding source and license notices.
The local bundle is ad-hoc signed; it is not Developer ID signed or notarized.
GitHub CI is provided separately; local results do not imply a successful hosted CI run.
