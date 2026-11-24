#!/usr/bin/env python3
"""Build a TSYN injected-data block for the timesync overlay app.

The block lives at offset 0x6000 inside the app's flash slot. When the
TIMESYNC app is opened on the radio, it validates the block and calls
the ABI's `utc_set` with the payload timestamp — this is how the web
flasher sets the radio's (RAM-only, power-off-volatile) UTC clock,
because the CPS protocol has no clock command. Contract (also in the
repo's APPS.md and the app's src/main.rs header):

    offset 0:  magic "TSYN"
    offset 4:  u8  version (= 1)
    offset 5:  u8  reserved (0)
    offset 6:  u16 LE payload length (= 8)
    offset 8:  u16 LE CRC-16/XMODEM over the 8 payload bytes
    offset 10: 6 reserved bytes (header is 16 bytes total)
    offset 16: payload (8 bytes):
                 utc_secs     u32 LE — seconds since 2000-01-01T00:00:00Z
                              (the utc_set argument; 0 is rejected)
                 written_unix u32 LE — Unix seconds at write time
                              (informational)

The flasher page builds this block in JS (buildTsynBlock in the page,
byte-identical to this builder) from Date.now() at click time:
utc_secs = unix - 946684800.

Usage:
    make_tsyn.py OUT.bin UTC_SECS WRITTEN_UNIX

Example (the simulator validation set):
    make_tsyn.py tsyn-demo.bin 816940800 1736942400
"""

import struct
import sys


def crc16_xmodem(data: bytes) -> int:
    crc = 0
    for b in data:
        crc ^= b << 8
        for _ in range(8):
            crc = ((crc << 1) ^ 0x1021) & 0xFFFF if crc & 0x8000 else (crc << 1) & 0xFFFF
    return crc


def build_tsyn_block(utc_secs: int, written_unix: int) -> bytes:
    payload = struct.pack("<II", utc_secs & 0xFFFFFFFF, written_unix & 0xFFFFFFFF)
    hdr = struct.pack("<4sBBHH6s", b"TSYN", 1, 0, len(payload), crc16_xmodem(payload), b"\x00" * 6)
    assert len(hdr) == 16
    return hdr + payload


def main() -> int:
    if len(sys.argv) != 4:
        print(__doc__)
        return 2
    out, utc_secs, written_unix = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
    block = build_tsyn_block(utc_secs, written_unix)
    with open(out, "wb") as f:
        f.write(block)
    print(f"wrote {out}: {len(block)} bytes (utc_secs={utc_secs}, written_unix={written_unix})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
