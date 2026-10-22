# Source provenance

This independent Rust storage prototype is not a complete port, decompilation or wrapper of the Windows installer.
The supplied `system_clone_v_1_6_1_src.zip` was examined for portability:

```
SHA-256 6244107095b69f2b0db401f55de34baf00b2098d7e15f102196aeaf57faec80c
```

It contains System Clone / ODIN, not the complete SSD Scope suite.
`src/ODIN/ODIN.cpp` identifies GPLv3; `src/ODIN/NTFSResize/ntfsresize.cpp` identifies GPLv2-or-later.
The archive also includes zlib and bzip2 notices. Win32 DeviceIoControl, NTFS and VSS backends
cannot be retargeted unchanged to macOS/APFS.

The pass-through functions `IsTSModelName_ATA` and `IsTSModelName_SCSI` in `src/ODIN/ODINDlg.cpp`
send ATA IDENTIFY DEVICE (`0xEC`) to inspect model-name words. The reviewed source search did
not locate a SMART temperature/health reader. These model-identification functions do not
establish that the ZIP contains SSD Scope's complete SMART implementation.

The original source, Windows code, icons and installer assets were not copied into the Rust app.
That archive is not redistributed or placed under the repository's MIT license.
Authoritative disclosure: https://transcend-info.com/Legal/?no=10
Product page: https://transcend-info.com/support/software/ssd-scope

REA 6.7.0, commit `f801b2fbfd28f4e3713adefddc7bd73db44c284c` in morluto/rea,
was used locally for static PE resource inspection. The installer was not executed.
REA did not supply app source. The native implementation, charts and geometric icons are original Rust code.

smartctl is a separately bundled program. Its exact unmodified source and GPL terms are supplied
as described in THIRD_PARTY_NOTICES.md.
