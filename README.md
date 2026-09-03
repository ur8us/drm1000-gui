# DRM1000 GUI

Cross-platform Rust desktop and command-line controller for the CML Micro
DRM1000 broadcast receiver module.

The application uses the DRM1000's 921,600-baud UART through a host virtual
serial adapter. It provides the same controls through a native `egui` desktop
interface and command-line subcommands.

## Features

- Frequency entry in Hz, kHz, or MHz and direct tuning
- DRM, AM wide, AM narrow, and FM controls
- Full-band scan, progress, result loading, service selection, and station up/down
- Volume and AM audio test-tone controls
- Continuous RSSI, DRM synchronization, MER, frequency-offset, service, and text display
- Four station preset recall/store slots from the CLI
- Guarded CMX918 register reads and writes
- Serial-port discovery and firmware-version probing
- Native Linux, Windows, and macOS support through `eframe` and `tokio-serial`

## Hardware Connection

The module UART uses `921600` baud, 8-N-1 and 3.1 V logic. It is not an RS-232
electrical interface. Use a compatible USB UART adapter, power the DRM1000
first, and do not drive its RX pin before module power is present.

The GUI deliberately does not automatically connect to an arbitrary serial
port. Select the adapter and press **Connect**. A device description containing
`DRM` or `DE9180` is preselected when available.

## Run

```bash
cargo run --bin drm1000-gui
```

List ports and probe a receiver:

```bash
cargo run -- ports
cargo run -- --port /dev/ttyUSB0 probe
```

Representative CLI operations:

```bash
drm1000-gui --port /dev/ttyUSB0 tune "15.2 MHz"
drm1000-gui --port /dev/ttyUSB0 mode drm
drm1000-gui --port /dev/ttyUSB0 scan 0
drm1000-gui --port /dev/ttyUSB0 stations
drm1000-gui --port /dev/ttyUSB0 volume 65
drm1000-gui --port /dev/ttyUSB0 status
drm1000-gui --port /dev/ttyUSB0 register-read 3d
drm1000-gui --port /dev/ttyUSB0 register-write 3d 55 --confirm
```

Use `drm1000-gui --help` and each subcommand's `--help` for the complete set.

## Build

Linux:

```bash
./build-linux.sh
```

Windows from a Linux host with MinGW-w64 installed:

```bash
./build-windows.sh
```

macOS uses the native Apple Rust target:

```bash
cargo build --release --bin drm1000-gui
```

## Development

The code is split into the vendor protocol codec (`src/protocol.rs`), async
serial connection (`src/device.rs`), GUI serial controller (`src/serial.rs`),
desktop UI (`src/app.rs`), and CLI/entry point (`src/main.rs`).

Run validation with:

```bash
cargo fmt --check
cargo test
cargo check
cargo check --target x86_64-pc-windows-gnu
```

Protocol tests cover fragmented/noisy response framing, status fields,
human-readable frequencies, and indexed scan-result labels.

For smoke tests without hardware, run `scripts/drm1000_simulator.py`; it prints
the pseudo-terminal path to pass through `--port`. The simulator covers session
initialization, probe, tuning, volume, status/RSSI, scan progress, and register
read transactions.

The vendor datasheet revision 2.2 and the DE9180/DRM1000 user manual revision
UM9180/1.0 are downloaded to the gitignored `assets/` directory. Derived protocol notes are tracked in
[`docs/serial-protocol.md`](docs/serial-protocol.md).

## Firmware

`probe` reports the module's firmware string. Firmware images and flashing
instructions are not public downloads; CML distributes protected support
material through its customer portal. This application will not attempt an
unverified flash. Compare a connected module's reported version with the
firmware supplied for that exact hardware revision by CML Micro. During initial
development, the only detected adapter (`/dev/ttyACM0`, a Raspberry Pi Debug
Probe UART) did not answer the documented `UART_INIT` request at 921,600 baud,
so no hardware version or update could be verified.

## License

MIT. See [LICENSE](LICENSE).
