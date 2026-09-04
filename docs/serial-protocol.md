# DRM1000 Serial Protocol Notes

Source: CML Micro `DRM1000 Broadcast Receiver Module` datasheet,
document revision `DRM1000/2.2`. The downloaded vendor PDF and extracted text
are kept in the gitignored `assets/` directory.

## Transport

- UART: `921600` baud, 8 data bits, no parity, 1 stop bit
- DRM1000 is DTE: pin 32 TXD, pin 33 RXD
- Logic high is 3.1 V; this is not an RS-232 electrical interface
- Do not drive RXD high before the DRM1000 is powered
- A session starts by sending the single byte `0x7F` (`UART_INIT`)
- Multibyte parameters and response fields are little-endian

Commands contain an opcode followed immediately by its fixed-size parameters.
Each response contains the echoed opcode, one error byte, a four-byte total
frame length (including the six-byte header), and optional payload data.

## Commands Used by This Application

| Opcode | Operation | Request data | Response data |
| --- | --- | --- | --- |
| `00` | Standby/restart | mode byte | none |
| `01` / `02` | Station up/down | none | none |
| `03` | Tune station ID | 24-bit ID in `u32` | none |
| `04` | Tune frequency | Hz as `u32` | none |
| `05` | Set mode | DRM=0, wide=1, narrow=2 | none |
| `06` | Band up | none | none |
| `09` | Get scanned stations | none | variable |
| `0A` | Get/status output packet | none | 409-byte payload |
| `0B` / `0C` | Recall/store preset | slot 0-3 | none |
| `0D` / `0E` | CMX918 read/write | address[, value] | value for read |
| `0F` | Continuous status output | boolean | none |
| `10` | Firmware version | none | ASCII version |
| `14` / `15` | Get frequency/mode | none | `u32` / byte |
| `18` / `19` | Raster station up/down | none | none |
| `1A` | Start scan | band selector, 0=all | none |
| `1D` / `1E` | Get/set volume | none / 0-100 | volume for get |
| `20` / `21` | Text output/service | boolean / none | UTF-8 message |
| `22` | RSSI | none | IEEE-754 `f32` dBm |
| `25` / `26` | Set/get screen state | boolean / none | state for get |
| `27` | Scanner progress | none | percent byte |
| `50` / `51` | Read/write persistent configuration | none / 255 bytes | 255 bytes for read |
| `60` | Audio test tone | boolean | none |
| `7F` | UART initialization | none | none |

The protocol's mode-set values `1` and `2` select wide/narrow analogue mode.
The receiver reports AM or FM according to the tuned band. Consequently the
GUI's FM button sends wide analogue mode, while the status packet remains the
authority for the resulting demodulator mode.

Persistent configuration bytes 35, 36, and 37 are signed DRM, AM, and FM audio
gain values respectively, each in the range `-20` to `+20 dB`. A write must
preserve all other bytes from a successful configuration read. New persistent
settings take effect after a receiver restart. These audio settings are
separate from the mode-specific CMX918 RF/IF AGC tables later in the same
configuration block.

Error 11 means the module has no valid stored configuration and is using its
built-in defaults. A gain write cannot preserve unreadable bytes. The
application therefore requires a separate explicit initialization choice and
constructs all 255 bytes from the defaults in datasheet revision 2.2, including
the MF capacitor LUT and all six CMX918 AGC profiles. Connection and discovery
never perform this initialization.

## Safety

CMX918 register writes are immediate and are not validated by the module. The
GUI requires an enable checkbox for each write; the CLI requires `--confirm`.
Persistent gain changes are also guarded because opcode `51` writes flash.
Firmware flashing is not part of this UART command set and must not be guessed
or attempted without the vendor's matching image and update procedure.
