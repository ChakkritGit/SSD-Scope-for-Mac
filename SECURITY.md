# Security

The dashboard does not run as root, install kernel drivers, disable SIP or change startup security.
Discovery, IORegistry sampling and ordinary USB SMART probes are read-only. There is no telemetry upload,
background firmware updater or raw erase/restore command.

The separate `scope-usb-smart capture-read` diagnostic requires explicit administrator authorization
and an unmounted, reviewed RTL9201 target. It temporarily captures the whole USB device and can
interrupt access. It is never launched by dashboard polling and never unmounts a drive itself.
Physical-disk/APFS/kernel mount checks, registry identity checks, fixed data-in command builders,
bounded transfers and driver restoration reduce risk; they do not make untested hardware safe.
Cancellation is cooperative. SIGKILL, power loss or a driver/backend failure can prevent cleanup;
reconnecting the SSD may be required. Do not run it from the SSD being captured.
See [the exact command allowlist and capture procedure](docs/DIRECT-USB.md).

Captured files contain drive health data. ATA serial/WWN fields are removed, but reports should
still be kept private. Offline report reads are bounded, reject symlinks and nonregular files,
and never interpret a missing SMART RETURN STATUS as passing health. Decoding refuses root.

Explicit GUI/CLI import binds a saved capture to a mounted volume UUID, reviewed USB bridge and
matching ATA capacity. This is a user-selected association, not cryptographic authentication of
the capture's origin. Cached reports are private 0600 files in an owned 0700 directory, bounded to
1 MiB/16 captures, and validated before offline decoding. Symlinks, FIFOs, public files, duplicate
UUID bindings and future timestamps are refused. A disk-number change does not reassign a reading
to another volume. Snapshot temperatures always carry snapshot/time labels and are never plotted
as new live temperature samples. Removing a saved snapshot does not modify the SSD's SMART data.

Benchmarks create an exclusive unpredictable temporary file in the selected directory, disable
its OS cache, verify read-back contents and remove only that owned file. Device paths and invalid
sizes are rejected; at least 1 GB headroom beyond the test size is required.

Backups create a private 0700 directory, never merge into an existing destination, and reject
destinations within the source after resolving symlink aliases. An exclusive rename marks completion.
Failed/cancelled copies remain .partial. The SHA-256 reader refuses symlink leaf targets and
verification detects symlink substitutions. Subprocesses use argument arrays, bounded execution
and bounded output, never shell commands containing user paths.

Security tests use owned temporary directories, synthetic SMART/telemetry fixtures and process
timeouts. They are regression checks, not an independent penetration test or a zero-vulnerability claim.
They do not establish safety against a hostile administrator or simultaneous malicious filesystem changes.
No tests write raw user disks. Live-device probes are read-only and documented separately.

Widget preferences are bounded JSON with schema validation. Reads refuse symlink leaf targets;
saves use an atomic private temporary file (0600) in an owner-only app settings directory (0700).
The app does not install a driver or alter Full Security to obtain missing SMART data.

Dependency advisories are checked with cargo-audit; results are in docs/VALIDATION.md.
The bundle is ad-hoc signed, not Developer ID notarized.

Do not post drive serial numbers or personal backup paths publicly. For a vulnerability, use GitHub
private reporting if enabled, or contact the repository maintainer privately before public disclosure.
