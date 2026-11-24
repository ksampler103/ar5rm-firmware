# AR-5RM web flasher — serial protocol spec

Precise, implementable spec for a JavaScript **Web Serial** implementation of the
three cable operations: factory backup read, bootloader probe, and flash.
Every value below was taken from the actual Python/C sources on clt-build-01
(`~/ar5rm-firmware/`), cited per section. Nothing here was run against a radio.

Source trees (commit hashes verified on the box 2026-11-24):

| Tree | Commit | Role |
|---|---|---|
| `Baofeng-UV-5RM-5RH-RE` (amoxu) | `cc03870e0e29d9ded34a55d907d00167e8e0bb6f` | `.BF` wrap tool, crypto |
| `bf-5rh-firmware-open` | `e52b2360af7b3152df72f1255682503a0555bb98` | `tools/flash.py`, `tools/cps_proto.py` |
| `UV-5RM-Radio-Mod` | `d605767763edda6b9bb094cfd25b885dddf8a892` | `FLASHING.md`, `source-changes/*.py` (the tested end-to-end path) |

**Per-operation source selection (the rule the web tool follows):**

- **Backup read** → mod's `source-changes/radio_read.py` (the stock-radio tool
  named in `FLASHING.md` step 1).
- **Probe** → mod's `source-changes/bootloader_probe.py`.
- **Flash** → mod's `source-changes/flash_bf.py` (see §5 for why, and how it
  relates to bf-5rh `tools/flash.py`).

Key paths on the box (mod files live under
`UV-5RM-Radio-Mod/UV-5RM Custom Firmware/source-changes/`):

- `radio_read.py`, `bootloader_probe.py`, `flash_bf.py`, `wrap_bf.py`,
  `backup_radio.py`, `cps_tool.py`
- bf-5rh: `bf-5rh-firmware-open/tools/flash.py`, `tools/cps_proto.py`
- amoxu: `Baofeng-UV-5RM-5RH-RE/reverse_engineering/uv5rm-wrap-tool/{main.c,encrypt.c,encrypt.h}`,
  `reverse_engineering/bao_fw_tool.py`

---

## 1. Serial settings (all operations)

All three operations use the **same baud rate: 115200**. There is no baud
switching between backup mode and bootloader mode.

| Parameter | Backup read (`radio_read.py`) | Probe (`bootloader_probe.py`) | Flash (bf-5rh `flash.py` / mod `flash_bf.py`) |
|---|---|---|---|
| Baud | 115200 (`radio_read.py:31`) | 115200 (`bootloader_probe.py:28`) | 115200 default, `--baud` overridable (`flash.py:290`); hard-coded 115200 (`flash_bf.py:38`) |
| Data bits / parity / stop | pyserial defaults = 8 / N / 1 (not set explicitly) | pyserial defaults = 8 / N / 1 | Explicitly `EIGHTBITS / PARITY_NONE / STOPBITS_ONE` (`flash.py:228-235`); `flash_bf.py` uses defaults |
| Flow control | Explicitly **off**: `rtscts=False, xonxoff=False` (`radio_read.py:31`) | Not set (pyserial default: off) | Not set (default: off) |
| DTR / RTS | **Not set by the code** | Set `True` after open (`bootloader_probe.py:29-30`) | Set `True` after open (`flash.py:236-237`, `flash_bf.py:39-40`) |
| Read timeout | 0.25 s per `read()` call (`radio_read.py:31`) | 0.05 s slice (`bootloader_probe.py:28`) | 0.03 s slice (`flash.py:26,234`; `flash_bf.py:38`) — overall frame deadline 2.0 s, see §4 |
| Port-open behaviour | `reset_input_buffer()`, then `sleep(0.01)` before first send (`radio_read.py:42-43`) | No reset at open; a 1.0 s idle-listen happens first (§3) | `reset_input_buffer()` before **every** (re)transmission (`flash.py:126`, `flash_bf.py:62`) |

Web Serial mapping: `port.open({ baudRate: 115200, dataBits: 8, parity: "none",
stopBits: 1, flowControl: "none" })`; for probe/flash also
`port.setSignals({ dataTerminalReady: true, requestToSend: true })` after open.
For backup read the Python code asserts neither signal — leave signals alone.

There are **no inter-chunk delays** in any tool: frames are written in a single
`write()` and the next frame is sent as soon as the previous ACK arrives
(backup read: next request immediately after the previous 68-byte response is
complete). Pacing is purely ACK-driven.

---

## 2. Operation A — factory backup read (STOCK radio, main screen)

Source: mod `radio_read.py` (whole file, 67 lines). Its docstring: "Read-only
dump of a Baofeng 5RM, following CHIRP's BF5RM download sequence." A `send()`
guard (`radio_read.py:34-38`) refuses to transmit anything except the handshake
bytes and `R` read frames — there is no write path. `FLASHING.md:47-54`
prescribes exactly this tool for a stock radio's first flash.

### 2.1 Which protocol the stock radio speaks

The stock radio speaks the **CHIRP / factory CPS protocol**, *not* the bf-5rh
CPS protocol:

- Stock ident string: **`PROGRAMBFNORMALU`** (16 bytes, `radio_read.py:10`).
- bf-5rh custom-firmware CPS ident: **`PROGRAMBF-5RH`** (13 bytes;
  `bf-5rh-firmware-open/tools/cps_proto.py:28`, mirrored in mod
  `source-changes/cps_tool.py`). That protocol (0xA6-framed, CRC'd) is spoken
  only by a radio *already running* bf-5rh-derived firmware. The mod's
  `backup_radio.py` uses it, and `FLASHING.md:39-45` assigns `backup_radio.py`
  to "radio already running the custom firmware" only.
- The web tool's backup operation **must use the stock protocol below**
  (`radio_read.py`). Do not send `PROGRAMBF-5RH` to a stock radio.

### 2.2 Exchange, step by step

All multi-byte integers in this protocol are **big-endian**
(`struct.pack(">BHB", …)`, `radio_read.py:57`).

1. **Ident.** Send the 16 ASCII bytes `PROGRAMBFNORMALU`. Read 1 byte.
   It must be **`0x06`** (ACK); anything else / no response → abort with
   "radio did not acknowledge" (`radio_read.py:44-48`). Radio must be on, at
   its main screen (`FLASHING.md:36-37,125`).
2. **Three "magic" exchanges** (`radio_read.py:11-13,49-52`). Responses are
   read but **not validated** by the tool (it prints them); a Web Serial
   implementation should read exactly the stated counts to keep the stream in
   sync, and may log them the same way:
   | # | Send | Read back |
   |---|---|---|
   | 1 | `0x46` (`'F'`, 1 byte) | 16 bytes |
   | 2 | `0x4D` (`'M'`, 1 byte) | 15 bytes |
   | 3 | 25 bytes: `53 45 4E 44 21 05 0D 01 01 01 04 11 08 05 0D 0D 01 11 0F 09 12 09 10 04 00` (starts with ASCII `SEND!`) | 1 byte |
3. **Block reads.** For each (start, size) region in order
   (`radio_read.py:14`):

   | Region start | Size |
   |---|---|
   | `0x0000` | `0x8040` (32,832) |
   | `0x9000` | `0x0040` (64) |
   | `0xA000` | `0x02C0` (704) |
   | `0xD000` | `0x0040` (64) |

   Total payload = **33,664 bytes**. Addresses are in the radio's CPS memory
   space (not raw flash addresses); regions are concatenated in the order
   above with no gaps or headers in the output file.
   
   Per block (block size `BLOCK = 0x40` = 64, `radio_read.py:15`):
   - Request: exactly 4 bytes — `0x52` (`'R'`), address as **u16 BE**,
     length as u8 = `0x40`. I.e. `52 <addr_hi> <addr_lo> 40`.
   - Response: exactly **68 bytes** (`BLOCK + 4`, `radio_read.py:58`): a
     4-byte header (echo of the request — the tool does not inspect it)
     followed by 64 data bytes. A short read at any block aborts the whole
     dump (`radio_read.py:59-60`). There is **no ACK/NAK, no checksum, and no
     retry** in this protocol as implemented — the per-read serial timeout is
     0.25 s.
   - Each 64-byte data block is passed through `crypt()` before being
     appended (see §2.3).
4. **Termination.** None — after the last block of the last region the tool
   just closes the port and writes the file. No end frame, no reboot command.
5. **Output file.** The concatenated, decrypted 33,664 bytes are written
   verbatim to the output path (`radio_read.py:62-65`) — this is
   `factory_memory.img` in `FLASHING.md:51`. The web tool should offer it as a
   download with exactly that content and size; assert `size === 33664`.

### 2.3 The backup `crypt()` transform

`radio_read.py:16,20-28`: key = ASCII **`CO 7`** (bytes `43 4F 20 37`; comment
in source: `_encrsym = 1`, i.e. CHIRP's obfuscation variant 1). Applied per
64-byte block, key index restarting at each block (`i % 4` over the block):
for each byte `b` with key byte `k = KEY[i % 4]`: if `k != 0x20` (space) and
`b ∉ {0x00, 0xFF}` and `b != k` and `b != k ^ 0xFF`, output `b ^ k`; else
output `b` unchanged. The transform is its own inverse. Note the web tool
stores the **decrypted** form (what `radio_read.py` writes), so apply
`crypt()` to received data exactly as above.

---

## 3. Operation B — bootloader probe

Source: mod `bootloader_probe.py` (whole file, 63 lines). Docstring: "Sends
only the HANDSHAKE frame; there is no write path here." Frame layout and CRC
are stated to match bf-5rh `tools/flash.py` — verified: the framing code is
identical in construction (`bootloader_probe.py:22-25` vs `flash.py:50-53`).

Precondition (operator action, `FLASHING.md:56-60`): radio off → hold **both
small side buttons next to PTT** → power on → screen stays black = flash mode.

Sequence (`bootloader_probe.py:28-61`):

1. Open port (115200, slice timeout 0.05 s), set `dtr = rts = True`.
2. **Idle listen, 1.0 s:** read and collect anything the radio sends
   unprompted (printed as "bytes heard before sending anything"). Not part of
   the pass/fail criterion.
3. Up to **5 attempts**: `reset_input_buffer()`, write the handshake frame
   (§4.2), then collect bytes for up to **2.0 s**, stopping early once a
   buffer starting with `0xAA` of ≥ 8 bytes has been received. The loop
   **breaks on the first attempt that returns any bytes at all**
   (`bootloader_probe.py:49-51`) — later attempts happen only on total
   silence.
4. **Success criterion — "BOOTLOADER ANSWERED"** (`bootloader_probe.py:52-59`):
   the reply's first byte is `0xAA`, total length ≥ 8, the frame parses as
   `cmd, args, len_hi, len_lo = reply[1:5]`, `n = (len_hi<<8)|len_lo`, the
   CRC16-XMODEM over `reply[1 : 5+n]` equals the big-endian u16 at
   `reply[5+n : 7+n]`, **and `args == 0x06` (ACK)**. If a CRC-valid frame
   arrives with `args != 0x06`, the verdict is "BOOTLOADER REFUSED THE
   HANDSHAKE"; no `0xAA`-leading reply at all → "NO BOOTLOADER RESPONSE".
   (For the expected handshake response, `n = 0`, so the full frame is
   exactly 8 bytes: `AA <cmd> 06 00 00 <crc_hi> <crc_lo> EF`. The probe code
   does not check the footer byte or the echoed `cmd` value; the web tool
   should check the footer too — `flash.py`'s reader only logs a bad footer
   but still accepts the frame, §4.3.)

---

## 4. Operation C — bootloader flash protocol

Sources: bf-5rh `tools/flash.py` (authoritative protocol implementation) and
mod `source-changes/flash_bf.py` (the tested flasher, §5). Where both exist
they agree; differences are listed in §5.

### 4.1 Frame format (`flash.py:10-11,50-53`; identical in `flash_bf.py:22-25`, `bootloader_probe.py:22-25`)

```
+--------+-----+----------+--------+--------+===========+---------+--------+
| 0xAA   | cmd | cmd_args | len_hi | len_lo | data[len] | crc_hi  | crc_lo | 0xEF |
+--------+-----+----------+--------+--------+===========+---------+--------+
  header   \___________ body ___________/              \_ CRC _/   footer
```

- `len` = data length, **big-endian u16**; frame total = `8 + len` bytes.
- **CRC = CRC16-XMODEM** over the **body only** (`cmd ‖ cmd_args ‖ len_hi ‖
  len_lo ‖ data`): polynomial `0x1021`, init `0x0000`, no reflection, no final
  XOR (`flash.py:40-47`). CRC is appended big-endian, before the `0xEF`
  footer.
- Worked example (from the code, computed 2026-11-24): the handshake frame is
  the 18 bytes `aa 01 00 00 0a 42 4f 4f 54 4c 4f 41 44 45 52 52 ab ef`
  (`BOOTLOADER` = 10 data bytes, CRC `0x52AB`).

### 4.2 Commands (`flash.py:13-19`)

| Cmd | Name in source | cmd_args | Data | Meaning |
|---|---|---|---|---|
| `0x01` | `CMD_HANDSHAKE` | `0` | ASCII `BOOTLOADER` (10 bytes) | Opens the session; must be ACKed before anything else (`flash.py:241`) |
| `0x04` | `CMD_UPDATE_DATA_PACKAGES` | `0` | 1 byte: number of 1024-byte packages region 1 will comprise | Declares region 1 (application @ `0x08001000+`) package count (`flash.py:246-253`) |
| `0x03` | `CMD_UPDATE` | package sequence number `seq & 0xFF`, starting at 0 | Exactly 1024 bytes (final package zero-padded, §4.4) | One data package of the currently declared region (`flash.py:187-198`) |
| `0x05` | `CMD_UPDATE_DATA_PACKAGES2` | `0` | 1 byte: package count | Declares region 2 (system-bootloader area @ `0x1FFFE400+`), then region 2 packages follow with `0x03` (`flash.py:256-268`) |
| `0x45` | `CMD_UPDATE_END` | `0` | empty | Ends the update; radio finalises/reboots (`flash.py:274-275`) |

Order of a full flash (`flash.py:240-276`): handshake → declare region 1
count (`0x04`) → all region 1 packages (`0x03`, seq 0…N−1) → *if region 2
non-empty:* declare region 2 count (`0x05`) → region 2 packages (`0x03`) →
end (`0x45`).

**Region limits** (`flash.py:21-23,211-226`): region 1 ≤ **`0xF000` (61,440)
bytes**, region 2 ≤ **`0x1000` (4,096) bytes** — bootloader-enforced;
oversize is rejected with error `0xE2`. Package count per region must fit in
one byte (≤ 255 packages); the count byte in `0x04`/`0x05` and the `0x03`
sequence byte both wrap at `0xFF`.

### 4.3 Responses, errors, retries (`flash.py:19,67-159`)

- Every command is answered with a frame in the same format. **ACK is not a
  cmd value — it is `cmd_args == 0x06`** in the response frame
  (`flash.py:138`). The response's `cmd` byte echoes or carries an error.
- Error codes (carried in the response frame's **cmd byte** when
  `cmd_args != 0x06`; `flash.py:28-33`): `0xE1` handshake/command-class
  error, `0xE2` sequence-number or length mismatch, `0xE3` flash write
  failed, `0xE5` unknown command.
- Reader behaviour (`flash.py:67-110`): scan byte-by-byte until `0xAA`, then
  read the 4-byte body header, `len` data bytes, 2 CRC bytes, 1 footer byte,
  all within one overall deadline of **2.0 s** (`FRAME_TIMEOUT_S`,
  `flash.py:24`) from frame start. A footer ≠ `0xEF` is only logged, not
  rejected; a CRC mismatch makes the frame "malformed".
- Retry policy (`flash.py:113-159`, `MAX_RETRIES = 5`): before each attempt,
  `reset_input_buffer()`, then write the whole frame in one `write()`. On an
  ACK → done. On error **`0xE2` → retry**; on any other error code → **abort
  immediately** ("denied"). On timeout or malformed/CRC-bad frame → retry.
  After 5 failed attempts → abort, *except* the end command (below).
- `CMD_UPDATE_END` is special (`flash.py:275`): only **2 attempts**, and
  `tolerate_no_response=True` — if no bytes were ever received, silence is
  accepted as success (the radio may reboot before replying; `flash_bf.py:94-95`
  prints exactly that interpretation).
- No artificial delays between packages: the next `0x03` is sent immediately
  after the previous package's ACK.

### 4.4 Package slicing

Chunk size **`CHUNK_SIZE = 1024`** (`flash.py:21`). Region payload is sliced
into 1024-byte packages; the **final package is padded with `0x00` bytes to
the full 1024** before sending (`flash.py:195-196`; `flash_bf.py:89` does the
same with `ljust(CHUNK, b"\x00")`). The bootloader therefore always receives
`ceil(len/1024) × 1024` bytes per region; the true length exists only in the
`.BF` header (§6).

---

## 5. Mod tools vs bf-5rh tools — disagreements and the flash recommendation

**Verified identical:** the mod's bundled `firmware-source/tools/flash.py` and
`tools/cps_proto.py` are **byte-for-byte identical** to bf-5rh upstream's
(diff clean, 2026-11-24). The mod's hand-written `source-changes/` tools are
independent minimal re-implementations of the same protocols.

**Actual disagreements / differences:**

1. **Which flasher the tested procedure uses.** `FLASHING.md:71-82` (step 3,
   the step that writes) invokes **`source-changes\flash_bf.py`**, *not*
   bf-5rh `tools/flash.py`. The tested end-to-end path is:
   `radio_read.py` → `bootloader_probe.py` → `flash_bf.py`.
2. **`flash_bf.py` is single-region only.** It asserts before opening the
   port (`flash_bf.py:31-34`): header region count `== 1`, `len_data == 0`,
   file length exactly `16 + len_fw`, `0 < len_fw ≤ 0xF000`, and — on the
   *encrypted* payload — `region[3] == 0x20` and `region[7] == 0x08` (the top
   bytes of the little-endian vector-table words, which survive because
   package 0 is never encrypted, §6). It has **no `0x05`/region-2 support**.
   `flash.py` supports two regions — but note `flash.py:269-272`: when
   region 2 is empty it prints that *skipping* `0x05` is "unverified against
   real hardware", i.e. upstream's own two-region flow is the historically
   exercised one and the one-region skip is not. The mod resolved that gap by
   writing `flash_bf.py` and testing the one-region path end-to-end.
3. **File-identity gate.** `flash_bf.py` requires a third CLI argument — the
   expected SHA-256 prefix of the `.BF` — and refuses to send a file whose
   hash doesn't start with it (`flash_bf.py:5,29-30`; `FLASHING.md:77-85`).
   `flash.py` has no equivalent check.
4. **Serial details.** `flash_bf.py` and `flash.py` agree on 115200 baud,
   0.03 s slice, DTR/RTS = True, 5 retries, 2.0 s frame timeout, and identical
   framing/CRC — no functional disagreement. `bootloader_probe.py` uses a
   0.05 s slice and its own 1 s idle-listen (probe-only behaviour).
   `radio_read.py` differs from both by design (different protocol, §2): 0.25 s
   timeout, explicit `rtscts=False/xonxoff=False`, and it never touches
   DTR/RTS.
5. **CPS confusion risk.** `backup_radio.py` / `cps_tool.py` (mod) and
   `cps_proto.py` (bf-5rh) implement the *custom-firmware* CPS
   (`PROGRAMBF-5RH`, 0xA6 frames). They are **not** interchangeable with the
   stock backup protocol (§2.1). `FLASHING.md` keeps them in separate steps
   for separate radio states; the web tool must do the same.

**Recommendation: mirror `flash_bf.py` for the web tool's flash operation.**
Reasons: (a) it is the flasher in the only documented, tested end-to-end
procedure (`FLASHING.md`); (b) our packages are one-region by construction
(wrap tool without `-d`, §6), which is exactly its asserted input shape;
(c) its pre-flight asserts (exact length, region limit, vector-table sanity)
and its SHA-prefix gate are safety properties worth keeping in the web UI —
the UI should require the operator to confirm the package SHA-256 (full hash
shown, prefix checked in code) before the Flash button arms; (d) its protocol
core is provably the same as `flash.py`'s (same framing, CRC, command set,
retry semantics), so nothing is lost. Revisit `flash.py`'s two-region flow
only if a future package ever carries a region 2 — and treat that as
untested until proven on the dev unit.

---

## 6. The `.BF` package format

Sources: amoxu `uv5rm-wrap-tool/main.c` (`BFHeader` struct, `wrapFW`/
`unwrapFW`), `encrypt.c`/`encrypt.h`; cross-checked against bf-5rh
`flash.py:_parse_bf` (`flash.py:162-184`), mod `flash_bf.py:31-34`,
`wrap_bf.py`, and `bao_fw_tool.py`. All agree.

### 6.1 Header — 16 bytes (`main.c:11-16`; parsed as `>BLL` + 7 ignored bytes)

| Offset | Size | Field | Value |
|---|---|---|---|
| 0 | 1 | `cntPart` — region count | `1` (firmware only) or `2` (firmware + data) |
| 1 | 4 | `lenFw` — region 1 length, **u32 big-endian** | length of the (encrypted) firmware payload that follows |
| 5 | 4 | `lenData` — region 2 length, **u32 big-endian** | `0` when `cntPart == 1` |
| 9 | 7 | reserved | **Ignored by every parser.** amoxu's wrap tool writes `0x00`×7 (`main.c:299-311`, struct zero-initialised); `bao_fw_tool.py:101-103` writes `0xFF`×7 and comments "Bytes 9-16 in header seem to be ignored by flasher". The factory NRF v0.14 file carries non-zero garbage here (`ff d8 8b 19 ac f7 d4`, measured 2026-11-24). Do not interpret these bytes. |

Layout after the header: region 1 payload (`lenFw` bytes, encrypted per
§6.2), then — only if `cntPart == 2` — region 2 payload (`lenData` bytes,
**stored unencrypted**: `wrapFW` writes the data part verbatim, `main.c:331-345`,
and `unwrapFW` reads it back verbatim). There are **no checksums, no version
fields, and no variant/target fields anywhere in the format** — the only
integrity mechanisms in the whole chain are the per-frame CRC16 on the wire
(§4.1) and operator-side hash checks (`flash_bf.py`'s SHA gate).

`flash.py:_parse_bf` validation (what the web tool should replicate when a
user picks a `.BF` file): file ≥ 16 bytes; `cntPart ∈ {1, 2}`; file actually
contains `lenFw` bytes after offset 16 and, if `cntPart == 2`, `lenData`
bytes after region 1. `flash_bf.py` is stricter (§5.2) — prefer its checks
for one-region files.

### 6.2 Encryption (obfuscation) of region 1

Applied in **1024-byte packages** (`PACKAGE_SIZE`, `encrypt.h:5`), package
index `i` counting from 0 over region 1 only (`main.c:313-328`,
`encrypt.c:13-19`):

- Packages **0 and 1, and the last two packages, are stored plain**
  (condition in code: transform only when `2 ≤ i < cntPkgs − 2`). Note this
  is *packages*, not bytes: "the first 2 KB are unencrypted" is the
  `i < 2` half of that rule; the tail rule is equally part of the format.
- For the remaining packages: if `i % 3 == 1`, XOR with key **`KDHT`**
  (`XOR_KEY1`); if `i % 3 == 2`, XOR with key **`RBGI`** (`XOR_KEY2`); if
  `i % 3 == 0`, the package stays plain even in the middle of the file.
- The XOR itself is conditional per byte (`encrypt.c:3-11`): with key byte
  `k = key[j % 4]` where `j` is the byte's offset **within its package**
  (key phase restarts every package), a byte `b` becomes `b ^ k` **only if**
  `b ∉ {0x00, 0xFF}` and `b != k` and `b != (k ^ 0xFF)`; otherwise `b` is
  stored unchanged. This makes the transform an involution — wrap and unwrap
  are the same operation (`wrap_bf.py` relies on this and self-checks against
  the factory images before wrapping, `wrap_bf.py:34-47`).
- The final partial package is transformed at its true length, not padded
  (lengths in header and file are the raw image lengths; padding to 1024
  happens only on the wire, §4.4).

### 6.3 Reference points (measured 2026-11-24)

- Factory `5RH_NRF_Fangti_V0.14_231124.BF` (amoxu repo): 60,704 bytes total,
  `cntPart = 2`, `lenFw = 59,392`, `lenData = 1,296`.
- Our M0 package: see `PACKAGE.md` in this folder — `cntPart = 1`,
  `lenFw = 57,492`, `lenData = 0`, header bytes
  `01 0000e094 00000000 00000000000000`.

---

## 7. Web Serial implementation notes (derived from the above)

- One port, three modes; the radio's *state* (stock main screen vs black-screen
  flash mode) selects the protocol, not anything the tool sends. UI must ask
  the operator which state the radio is in, exactly as `FLASHING.md` sequences
  it: backup (stock, main screen) → power-cycle into flash mode → probe →
  flash.
- Readers: implement two stream parsers — (a) fixed-length reader for the
  backup protocol (1-byte ACK, 16/15/1-byte magic replies, 68-byte block
  replies, 0.25 s per-read timeout, abort on short read); (b) frame scanner
  for the bootloader protocol (hunt `0xAA`, parse `len`, verify CRC, 2.0 s
  frame deadline, resync by discarding non-`0xAA` bytes as `flash.py:72-78`
  does).
- Writers: single `write()` per frame; `reader.cancel()`/buffer reset between
  attempts equivalent to `reset_input_buffer()` — discard all pending input
  before each (re)transmission.
- Retry engine for flash: 5 attempts per command, retry only on timeout /
  malformed / error `0xE2`; abort on `0xE1`/`0xE3`/`0xE5`; end command gets
  2 attempts and silence-is-success.
- File handling: validate a chosen `.BF` with the `flash_bf.py` assert set
  (§5.2) plus a SHA-256 display/confirm step before enabling Flash; offer the
  backup result as a 33,664-byte download named `factory_memory.img`.
- Safety gates (project rules, unchanged): backup must complete before flash
  arms; probe must return "BOOTLOADER ANSWERED" in the same session; the
  operator's explicit per-flash confirmation is required. Nothing in this
  spec authorises a flash by itself.
