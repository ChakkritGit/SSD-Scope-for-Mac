# Corresponding source for bundled smartctl

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
