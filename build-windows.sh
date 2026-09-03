#!/usr/bin/env bash
set -euo pipefail
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu --bin drm1000-gui
