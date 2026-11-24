#!/usr/bin/env python3
"""Build an RPBK injected-data block for the rptbook overlay app.

The block lives at offset 0x6000 inside the app's flash slot and, when
present and CRC-valid, replaces the app's baked-in directory. Contract
(also in the repo's APPS.md and the app's src/main.rs header):

    offset 0:  magic "RPBK"
    offset 4:  u8  version (= 1)
    offset 5:  u8  reserved (0)
    offset 6:  u16 LE record count (max 272)
    offset 8:  u16 LE CRC-16/XMODEM over the record bytes only
    offset 10: 6 reserved bytes (header is 16 bytes total)
    offset 16: records, 30 bytes each:
                 name[16]  ASCII, NUL/space padded
                 rx_hz     u32 LE (0 ends the list early)
                 tx_hz     u32 LE (0 = same as rx)
                 tx_tone   u16 LE, firmware SubaudioCode encoding:
                             0 = none, 1..105 DCS normal (index+1),
                             106..250 DCS inverted, >250 CTCSS in
                             tenths of a Hz (1413 = 141.3 Hz)
                 rx_tone   u16 LE, same encoding
                 flags     u8: bit0 duplex (tx != rx), bit1 TX tone set,
                             bit2 RX tone set, bit3 RX-only

Usage:
    make_rpbk.py OUT.bin NAME RX_HZ TX_HZ TX_TONE RX_TONE [NAME ...]

Example (the demo set used by scenarios/rptbook-injected.txt):
    make_rpbk.py rpbk-demo.bin \\
        "SIM ALPHA"  146520000 146520000 0    0 \\
        "SIM BRAVO"  446000000 441000000 1413 0 \\
        "SIM CHARLIE" 462562500 467562500 0   0
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


def main() -> int:
    args = sys.argv[1:]
    if len(args) < 6 or (len(args) - 1) % 5 != 0:
        print(__doc__)
        return 2
    out = args[0]
    fields = args[1:]
    records = bytearray()
    count = 0
    for i in range(0, len(fields), 5):
        name, rx, tx, ttone, rtone = fields[i:i + 5]
        rx_hz, tx_hz = int(rx), int(tx)
        tx_tone, rx_tone = int(ttone), int(rtone)
        flags = 0
        if tx_hz not in (0, rx_hz):
            flags |= 0x01
        if tx_tone:
            flags |= 0x02
        if rx_tone:
            flags |= 0x04
        name_b = name.encode("ascii")[:16].ljust(16, b"\x00")
        # 29 bytes of fields + 1 pad byte = the app's 30-byte record.
        records += struct.pack(
            "<16sIIHHBx", name_b, rx_hz, tx_hz, tx_tone, rx_tone, flags
        )
        count += 1
    block = bytearray()
    block += b"RPBK"
    block += struct.pack("<BBHH", 1, 0, count, crc16_xmodem(bytes(records)))
    block += b"\x00" * 6  # header is 16 bytes total
    block += records
    with open(out, "wb") as f:
        f.write(block)
    print(f"wrote {out}: {count} records, {len(block)} bytes")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
