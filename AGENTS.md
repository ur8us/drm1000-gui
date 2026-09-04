# AGENTS.md

## Repository Intent

This repository contains a native Rust GUI and CLI for the CML Micro DRM1000
broadcast receiver module. It targets Linux, Windows, and macOS.

## Current Architecture

- `src/protocol.rs` owns command encoding, framed-response decoding, status,
  station and persistent-configuration parsing, and vendor error descriptions.
- `src/device.rs` owns one asynchronous serial connection and transactions.
- `src/serial.rs` runs the GUI serial worker and publishes snapshots through a
  Tokio watch channel.
- `src/app.rs` owns the `egui` interface and sends typed requests to the worker.
- `src/main.rs` selects the GUI when no subcommand is given and implements CLI
  operations otherwise.
- `build-linux.sh`, `build-windows.sh`, and `build-macos.sh` provide release
  builds. The macOS script is intentionally native-only because it requires the
  Apple SDK.
- `scripts/drm1000_simulator.py` provides a pseudo-terminal test double on Unix
  for initialization, probe, tuning, volume, persistent audio gain,
  status/RSSI, progress, and CMX918 register-read transactions.

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
- Keep host wiring accurate: adapter RXD to DRM1000 pin 32 (`UART_TX`), adapter
  TXD to pin 33 (`UART_RX`), and adapter GND to a module ground such as pin 30.
- Do not suggest powering the bare module from an arbitrary USB-UART adapter.
  The main receiver uses a separate nominal 3.1 V supply on pin 7. The host
  must not drive pin 33 before the module supply is stable.
- Preserve the guarded register-write UX: GUI checkbox and CLI `--confirm`.
- Preserve all bytes except offsets 35-37 when changing persistent DRM/AM/FM
  audio gain. Gain writes must remain explicit and guarded because they update
  flash, and the UI must state that a receiver restart is required.
- If configuration read returns error 11, allow a write only after the user
  separately chooses initialization from the complete DRM1000/2.2 defaults.
  Never initialize persistent configuration during connect, probe, or read.
- Do not present audio gain boost as RF gain. RF/IF gain uses the receiver's
  mode-specific CMX918 AGC configuration and has no safe scalar control.
- Update frequency and volume editors only when their corresponding readback
  generation changes. Periodic status snapshots must not overwrite active user
  edits. A dirty frequency draft remains authoritative until Tune is submitted.
- Preserve legacy scan support for firmware `v0.17`: poll demodulator mode and
  station results while treating scanner error 13 as an expected in-progress
  response and no-station error 4 as completion. Scanner percentage is
  optional.
- Disable persistent status/text output on clean disconnect and quiesce stale
  status output before sending `UART_INIT` on a new connection.
- The module audio test tone works only in AM mode. Do not present it as
  available at VHF, where analogue mode resolves to FM.
- Do not add firmware flashing without an authenticated vendor image and the
  matching documented update process.
- The hardware probe on 2026-09-03 reported firmware
  `CCDRM-drm1000-prod-v0.17-20240515161312` through `/dev/ttyACM0`. Treat this
  as observed device state, not as the current vendor release version.
- Update this file and `README.md` whenever source behavior, architecture, or
  developer workflow changes.

## Validation

Run the smallest useful set, expanding before release:

```bash
cargo fmt --check
cargo test
cargo check
cargo check --target x86_64-pc-windows-gnu
bash -n build-linux.sh build-windows.sh build-macos.sh
```

Protocol tests should retain coverage for fragmented/noisy framing, the fixed
409-byte status payload, scan label-table indexing, field-specific editor
generations, persistent audio gain byte preservation, and legacy scan
completion.

## Repository Hygiene

- Track `Cargo.lock` because this is an application.
- Keep downloaded vendor documentation and firmware under ignored `assets/`.
- Track derived protocol notes under `docs/`.
