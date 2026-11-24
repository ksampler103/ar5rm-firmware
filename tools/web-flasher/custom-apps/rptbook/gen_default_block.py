#!/usr/bin/env python3
"""Build the DEFAULT RPBK block for the rptbook overlay app.

The app itself no longer carries the owner's 165-channel directory in
its binary (a resident overlay segment must fit ARENA_SIZE = 8192 B;
the table alone was ~4.6 KB). Instead the web flasher writes this
block at slot offset 0x6000 on every RPTBOOK install:

  - no ZIP entered  -> this default block (the owner's own channels,
                       from default_channels.json, extracted from the
                       original baked.rs table)
  - ZIP entered     -> a ZIP-built block with nearby repeaters

Block contract (see APPS.md and src/main.rs): 16-byte header, magic
"RPBK", version 1, header byte 5 = SOURCE (0/1 = local/ZIP data, the
app titles it "RPTBOOK (ZIP)"; 2 = this owner-default block, plain
"RPTBOOK" title), u16 LE count, u16 LE CRC-16/XMODEM over the record
bytes, then 30-byte records (name[16], rx_hz u32 LE, tx_hz u32 LE
(0 = same as rx), tx_tone u16, rx_tone u16 in firmware SubaudioCode
encoding, flags u8, pad u8). Record packing is byte-identical to
tools/simulator/tools/make_rpbk.py.

Usage: gen_default_block.py OUT.bin [channels.json]
"""

import json
import struct
import sys
from pathlib import Path

SOURCE_DEFAULT = 2


def crc16_xmodem(data: bytes) -> int:
    crc = 0
    for b in data:
        crc ^= b << 8
        for _ in range(8):
            crc = ((crc << 1) ^ 0x1021) & 0xFFFF if crc & 0x8000 else (crc << 1) & 0xFFFF
    return crc


def build_block(entries, source: int) -> bytes:
    records = bytearray()
    for e in entries:
        rx_hz = int(e["rx_hz"])
        tx_hz = int(e["tx_hz"])
        tx_tone = int(e["tx_tone"])
        rx_tone = int(e["rx_tone"])
        flags = 0
        if tx_hz not in (0, rx_hz):
            flags |= 0x01
        if tx_tone:
            flags |= 0x02
        if rx_tone:
            flags |= 0x04
        flags |= int(e.get("flags", 0)) & 0x08  # carry RX-only bit
        name_b = str(e["name"]).encode("ascii")[:16].ljust(16, b"\x00")
        records += struct.pack(
            "<16sIIHHBx",
            name_b,
            rx_hz,
            0 if tx_hz == rx_hz else tx_hz,
            tx_tone,
            rx_tone,
            flags,
        )
    block = bytearray()
    block += b"RPBK"
    block += struct.pack("<BBHH", 1, source, len(entries), crc16_xmodem(bytes(records)))
    block += b"\x00" * 6
    block += records
    return bytes(block)


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    out = sys.argv[1]
    src = (
        Path(sys.argv[2])
        if len(sys.argv) > 2
        else Path(__file__).with_name("default_channels.json")
    )
    entries = json.loads(src.read_text())
    block = build_block(entries, SOURCE_DEFAULT)
    Path(out).write_bytes(block)
    print(f"wrote {out}: {len(entries)} records, {len(block)} bytes, source=2 (default)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
