#!/bin/bash
set -euo pipefail
PROJECT="$(cd "$(dirname "$0")/.." && pwd)"
LOCAL_TOOLCHAIN="$(dirname "$PROJECT")/.toolchain"
if [ -x "$LOCAL_TOOLCHAIN/cargo/bin/cargo" ]; then
  export CARGO_HOME="$LOCAL_TOOLCHAIN/cargo"
  export RUSTUP_HOME="$LOCAL_TOOLCHAIN/rustup"
  export PATH="$CARGO_HOME/bin:$PATH"
fi
cd "$PROJECT"
if [ -f "$PROJECT/runtime/libusb-link.toml" ]; then
  exec cargo --config "$PROJECT/runtime/libusb-link.toml" "$@"
fi
exec cargo "$@"
