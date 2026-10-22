# Corresponding sources

## libusb 1.0.30

Unmodified official release:
https://github.com/libusb/libusb/releases/download/v1.0.30/libusb-1.0.30.tar.bz2

SHA-256: `fea36f34f9156400209595e300840767ab1a385ede1dc7ee893015aea9c6dbaf`

LGPL-2.1-or-later; full text in LIBUSB-COPYING and the archive's COPYING.
`../scripts/build-libusb.sh` verifies the digest and builds a shared library under runtime/libusb.
It selects the portable pipe fallback for the macOS 14 deployment target and bounds libtool's
command length so building also works without sysctl access. No source patches are applied.
It generates a local Cargo links override; use scripts/cargo.sh after this build.
The bundle preserves the source, license and script under Contents/Resources/libusb-sources.
Copy that directory somewhere writable before rebuilding. Replacement instructions are in
../docs/DIRECT-USB.md. No kernel extension or global installation is performed.

## smartctl 7.5

smartmontools-7.5.tar.gz is the unmodified source from:
https://downloads.sourceforge.net/project/smartmontools/smartmontools/7.5/smartmontools-7.5.tar.gz

SHA-256: `690b83ca331378da9ea0d9d61008c4b22dde391387b9bbad7f29387f2595f76e`

License: GPL-2.0-or-later. Full terms: SMARTMONTOOLS-COPYING and the archive's COPYING.
No patches are applied. ../scripts/build-smartctl.sh checks the digest, configures Darwin,
builds only smartctl and copies it into the project-local runtime folder. It installs no system tool,
daemon or driver. Xcode Command Line Tools are required.

Keep the source archive, license and script with binary distributions. The app bundle preserves
these under Contents/Resources/smartmontools-sources; copy that directory somewhere writable
before running its build script.
