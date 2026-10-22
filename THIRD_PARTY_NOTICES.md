# Third-party notices

The original Rust application and geometric icons are MIT licensed. Third-party software is not
relicensed by the application's LICENSE file.

## smartmontools / smartctl 7.5

Project: https://www.smartmontools.org/ and https://github.com/smartmontools/smartmontools

License: GPL-2.0-or-later; full preserved text: `sources/SMARTMONTOOLS-COPYING`.
The app invokes a separate executable through CLI arguments and JSON output, and does not link
smartmontools implementation code into the Rust application.

The unmodified corresponding source is `sources/smartmontools-7.5.tar.gz`, SHA-256
`690b83ca331378da9ea0d9d61008c4b22dde391387b9bbad7f29387f2595f76e`.
There are no patches. `scripts/build-smartctl.sh` provides configuration/build instructions.
The app bundle includes this source, COPYING, the script and executable build information.
Keep these together when redistributing the bundle.

## Rust dependencies

Versions are locked in Cargo.lock. Source notices and license texts are preserved under
THIRD_PARTY_LICENSES, with package/version/SPDX metadata in INDEX.json. This includes
egui/eframe, egui_plot, wgpu, muda, objc2 and other linked crates.
The original code-generated artwork is not a Transcend logo.
For packages that publish only an SPDX license declaration and no license file, the index identifies
that limitation and preserves original package metadata alongside the standard SPDX license text.
Those templates do not change the upstream license grant.

## Transcend System Clone / ODIN reference material

The supplied archive is reference-only and not incorporated into the Rust app. Its original
licenses include GPLv3 (ODIN), GPLv2-or-later (NTFSResize), zlib and bzip2 terms.
Transcend's Windows SSD Scope disclosure also lists BSD 3-Clause and GPLv3 components:
https://transcend-info.com/Legal/?no=10

The original archive and Windows installer are not redistributed by this repository.
See SOURCE_PROVENANCE.md. Nothing here grants a right to relicense that original code under MIT.
