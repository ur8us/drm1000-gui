# AGENTS.md

## Repository Intent

This repository contains a native Rust GUI and CLI for the CML Micro DRM1000
broadcast receiver module. It targets Linux, Windows, and macOS.

## Current Architecture

- `src/protocol.rs` owns command encoding, framed-response decoding, status and
  station parsing, and vendor error descriptions.
- `src/device.rs` owns one asynchronous serial connection and transactions.
- `src/serial.rs` runs the GUI serial worker and publishes snapshots through a
  Tokio watch channel.
- `src/app.rs` owns the `egui` interface and sends typed requests to the worker.
- `src/main.rs` selects the GUI when no subcommand is given and implements CLI
  operations otherwise.
- `scripts/drm1000_simulator.py` provides a pseudo-terminal test double on Unix
  for initialization, probe, tuning, volume, status/RSSI, progress, and CMX918
  register-read transactions.

## Working Conventions

- Preserve unrelated user changes.
- Use `apply_patch` for manual edits.
- Prefer `rg` and `rg --files` for search.
- Keep source ASCII unless a file already requires Unicode.
- Keep protocol and serial I/O separate from GUI rendering.
- Keep hardware operations explicit; do not write firmware or registers as
  part of discovery or connection.
- Treat `docs/serial-protocol.md`, the vendor DRM1000/2.2 datasheet, and the
  DE9180 user manual in ignored `assets/` as the protocol references.
- Keep the serial default at 921,600 baud, 8-N-1. The DRM1000 UART uses 3.1 V
  logic and must not be described as electrically compatible with RS-232.
- Preserve the guarded register-write UX: GUI checkbox and CLI `--confirm`.
- Do not add firmware flashing without an authenticated vendor image and the
  matching documented update process.
- Update this file and `README.md` whenever source behavior, architecture, or
  developer workflow changes.

## Validation

Run the smallest useful set, expanding before release:

```bash
cargo fmt --check
cargo test
cargo check
cargo check --target x86_64-pc-windows-gnu
```

Protocol tests should retain coverage for fragmented/noisy framing, the fixed
409-byte status payload, and scan label-table indexing.

## Repository Hygiene

- Track `Cargo.lock` because this is an application.
- Keep downloaded vendor documentation and firmware under ignored `assets/`.
- Track derived protocol notes under `docs/`.
