#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "build-macos.sh must be run on macOS with Xcode Command Line Tools installed" >&2
    exit 1
fi

cargo build --release --bin drm1000-gui
