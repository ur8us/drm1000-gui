#!/usr/bin/env python3
"""Small pseudo-terminal DRM1000 simulator for host-side smoke tests."""

import os
import pty
import struct


PARAMETER_LENGTHS = {
    0x00: 1, 0x03: 4, 0x04: 4, 0x05: 1, 0x0B: 1, 0x0C: 1,
    0x0D: 1, 0x0E: 2, 0x0F: 1, 0x13: 1, 0x16: 1, 0x1A: 4,
    0x1B: 1, 0x1E: 1, 0x20: 1, 0x23: 1, 0x25: 1, 0x51: 255,
    0x60: 1, 0x61: 1,
}


def frame(opcode, payload=b"", error=0):
    return bytes((opcode, error)) + struct.pack("<I", 6 + len(payload)) + payload


def main():
    master, slave = pty.openpty()
    print(os.ttyname(slave), flush=True)
    frequency = 1_000_000
    mode = 0
    volume = 50
    buffer = bytearray()

    while True:
        buffer.extend(os.read(master, 4096))
        while buffer:
            opcode = buffer[0]
            parameter_length = PARAMETER_LENGTHS.get(opcode, 0)
            if len(buffer) < 1 + parameter_length:
                break
            parameters = bytes(buffer[1:1 + parameter_length])
            del buffer[:1 + parameter_length]
            payload = b""
            if opcode == 0x04:
                frequency = struct.unpack("<I", parameters)[0]
            elif opcode == 0x05:
                mode = parameters[0]
            elif opcode == 0x0A:
                payload = bytearray(409)
                payload[0] = mode
                payload[2] = 1
                payload[382 + 20:382 + 24] = struct.pack("<f", -71.5)
                payload = bytes(payload)
            elif opcode == 0x0D:
                payload = bytes((0x42,))
            elif opcode == 0x10:
                payload = b"sim-2.2.0"
            elif opcode == 0x14:
                payload = struct.pack("<I", frequency)
            elif opcode == 0x15:
                payload = bytes((mode,))
            elif opcode == 0x1D:
                payload = bytes((volume,))
            elif opcode == 0x1E:
                volume = parameters[0]
            elif opcode == 0x22:
                payload = struct.pack("<f", -71.5)
            elif opcode == 0x27:
                payload = bytes((100,))
            os.write(master, frame(opcode, payload))
            if opcode == 0x0F and parameters == b"\x01":
                status = bytearray(409)
                status[0] = mode
                status[2] = 1
                status[382 + 20:382 + 24] = struct.pack("<f", -71.5)
                os.write(master, frame(0x0A, status))


if __name__ == "__main__":
    main()
