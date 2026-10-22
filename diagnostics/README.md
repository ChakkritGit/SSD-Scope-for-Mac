# Read-only Apple interface diagnostics

These are original MIT diagnostic sources, separate from the Rust app. They use typed C interfaces
from Apple's SDK and do not contain a SMART reader. They query cached descriptors or interface
availability, without opening/capturing/resetting devices, claiming interfaces, obtaining exclusive
access, creating SCSI tasks or sending commands to the disk.

Build and run as a regular user in Terminal with Xcode Command Line Tools. Replace `diskN` with
the current physical disk identifier shown by `diskutil list physical`; identifiers can change.

```sh
(
  set -e
  scope_probe_dir="$(mktemp -d "${TMPDIR:-/tmp}/scope-readonly-probes.XXXXXX")"
  trap 'rm -rf "$scope_probe_dir"' EXIT
  clang -Wall -Wextra -Wno-deprecated-declarations -framework IOKit -framework CoreFoundation \
    diagnostics/usb-descriptor-probe.c -o "$scope_probe_dir/usb-descriptors"
  clang -Wall -Wextra -Wno-deprecated-declarations -framework IOKit -framework CoreFoundation \
    diagnostics/scsi-interface-probe.c -o "$scope_probe_dir/scsi-interface"
  "$scope_probe_dir/usb-descriptors"
  "$scope_probe_dir/scsi-interface" diskN
)
```

The USB probe filters to VID/PID `0bda:9201`. Neither probe prints serial numbers.
Running inside a development sandbox may deny plug-in creation even when Terminal succeeds.
The recorded results and their limits are in [NATIVE-TRANSPORT.md](../docs/NATIVE-TRANSPORT.md).
