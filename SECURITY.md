# Security

The app does not run as root, install kernel drivers, disable SIP or change startup security.
Discovery, IORegistry sampling and USB SMART probes are read-only. There is no telemetry upload,
background firmware updater or raw erase/restore command.

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
