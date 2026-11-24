# bf-5rh CPS protocol — spec for the web flasher's Step 4

Precise, implementable spec for talking to a radio **already running bf-5rh
firmware** (our build of upstream `sophiel-meow/bf-5rh-firmware-open`
@ `e52b2360af7b3152df72f1255682503a0555bb98`): reading/writing the settings
record (band lock) and pushing overlay app packages. This is a *different
protocol* from both the stock-radio backup protocol and the bootloader flash
protocol in `PROTOCOL.md` — different ident, different framing, different
radio state. Do not mix them.

Sources (all read on clt-build-01, 2026-11-24):

| Source | Role |
|---|---|
| `bf-5rh-firmware-open/src/cps/mod.rs` (375 lines) | **Radio side — authoritative** |
| `bf-5rh-firmware-open/src/cps/frame.rs` (179 lines) | Frame format, radio side |
| `bf-5rh-firmware-open/src/flash_map.rs` | Address map, Settings record layout |
| `bf-5rh-firmware-open/src/device/radio.rs` | `BandLock` profiles and TX ranges |
| `bf-5rh-firmware-open/abi/src/lib.rs`, `abi/build.rs` | App package header, `MAGIC`, `BUILD_HASH` |
| `bf-5rh-firmware-open/tools/cps_proto.py` | Host reference (`CpsSession`) |
| `bf-5rh-firmware-open/tools/push_app.py` | App-push driver (thin wrapper over `cps_proto`) |
| `UV-5RM-Radio-Mod/…/firmware-source/tools/cps_proto.py` | **Byte-identical** to upstream's (diff-verified) |
| `UV-5RM-Radio-Mod/…/source-changes/cps_tool.py` | Independent host re-implementation; agrees on header/ACK/ident/framing |

Where sources disagree, the radio-side firmware source wins. Disagreements
found are listed in §9.

---

## 1. Transport and radio state

- **Radio state:** ON, running bf-5rh, at its normal standby screen. Not
  flash mode, not stock firmware (a stock radio never answers this ident —
  it speaks `PROGRAMBFNORMALU`, see `PROTOCOL.md` §2).
- **Serial:** 115200 baud, 8N1, no flow control. Firmware side:
  `src/main.rs:26` `const BAUD: u32 = 115_200` (USART1, divider 1042 at
  120 MHz SCLK). Host side: `cps_proto.CpsSession(port, baud=115200)`,
  pyserial defaults (8N1, no flow control). `cps_proto.py` never touches
  DTR/RTS explicitly (pyserial leaves them asserted at open, same as the
  other tools). The web page's existing behaviour — assert DTR+RTS after
  open — is compatible.
- On entering a CPS session the radio turns its RF off and shows a "CPS /
  PROGRAMMING" screen (`src/cps/mod.rs:run_session`, `src/ui/cps.rs`).

## 2. Handshake (session entry)

- The radio's main loop **continuously scans the UART byte stream** for the
  13 ASCII bytes **`PROGRAMBF-5RH`** (`src/cps/mod.rs:HandshakeDetector`;
  not framed, no header/CRC). On a full match it enters `run_session` and
  immediately sends back **one byte: `0x06` (ACK)**.
- Host behaviour (`cps_proto._handshake`): up to **5 attempts**; before each,
  discard pending input; write the 13 ident bytes in one write; wait up to
  **1.0 s** for the single ACK byte. First `0x06` wins. After the ACK the
  per-read slice timeout drops to 0.03 s and framed commands (§3) are used.
- The handshake returns **no identity information** — just `0x06`. Anything
  the UI displays about the radio (settings, slot contents) must be read
  with commands after the handshake.
- **Session lifetime — critical:** the session has a **2,000 ms idle
  timeout** (`src/cps/mod.rs:IDLE_TIMEOUT_MS`). If no byte arrives for 2 s,
  the radio reboots itself out of the session (`SCB::sys_reset`). The host
  must keep commands flowing with < 2 s gaps *between frames*; long
  operations (app erase, §7) are single frames the radio answers when done,
  so the gap that matters is host-side thinking time between commands.
  `CMD_END` (§4) ends the session politely: the radio ACKs, drains its TX
  buffer, and reboots. Either way, **the radio reboots when a CPS session
  ends** — warn the operator.

## 3. Frame format (`src/cps/frame.rs`, mirrored by `cps_proto.encode_frame`)

```
+--------+-----+----------+----------+===========+---------+---------+
| 0xA6   | cmd | addr_hi  | addr_lo  | len_hi    | len_lo  | data…   | crc_hi | crc_lo |
+--------+-----+----------+----------+===========+---------+---------+
```

- Header byte **`0xA6`** (not the bootloader's `0xAA`, and there is **no
  footer byte**).
- `addr` and `len` are **big-endian u16**. Frame total = **8 + len** bytes.
- **CRC = CRC16-XMODEM** (poly `0x1021`, init `0x0000`, no reflection, no
  final XOR — the same CRC as the bootloader protocol) over
  `cmd ‖ addr_hi ‖ addr_lo ‖ len_hi ‖ len_lo ‖ data` (i.e. everything after
  the `0xA6` header, before the CRC), appended big-endian.
- **Maximum data length: 512 bytes** (`frame::MAX_DATA`). The radio's frame
  reader *silently drops* a frame whose declared len > 512 and resyncs on
  the next `0xA6`.
- Worked example (computed from this spec; the page's framing code is
  verified against it in the build notes): a settings read,
  `CMD_READ` at addr `0x9000` with no data, is the 8 bytes
  **`a6 52 90 00 00 00 97 2e`** (CRC `0x972E` over `52 90 00 00 00`;
  recomputed independently when this spec was written).

### Responses and errors

- Every command is answered with a frame in the same format: response `cmd`
  echoes the request `cmd`, `addr` echoes the request `addr`, and `data`
  carries the payload (reads) or is empty (write ACKs).
- **Errors** are frames with `cmd = 0xEE` (`CMD_ERROR`) and a 1-byte data
  payload (`frame.rs`, `cps/mod.rs:send_error`):
  | Code | Meaning | Raised when |
  |---|---|---|
  | `0x01` | bad CRC | a received frame's CRC didn't match (response addr = the bad frame's addr) |
  | `0x02` | bad address | address not readable/writable, wrong exact length on a write, app-write with no preceding erase, bad slot/offset |
  | `0x03` | bad length | **also the radio's answer to any unknown command byte** (`cps/mod.rs` default match arm) |
- Host retry policy (`cps_proto.send`): before each attempt, discard pending
  input, write the whole frame in one write, wait up to **2.0 s**
  (`FRAME_TIMEOUT_S`) for a CRC-valid response (the reader scans for `0xA6`,
  discarding other bytes). On a `0xEE` response → **abort immediately**
  (raise, no retry). On timeout / no CRC-valid frame → retry, up to
  **5 attempts** (`MAX_RETRIES`), then fail. Retrying is safe for every
  command used here: reads are pure, writes rewrite the same bytes, app
  erase re-erases the same slot.

## 4. Command set (`src/cps/frame.rs`)

| Cmd | Name | addr field means | data |
|---|---|---|---|
| `0x52` `'R'` | `CMD_READ` | logical address (§5) | request: none; response: the record |
| `0x57` `'W'` | `CMD_WRITE` | logical address (§5) | the record, exact length (§5) |
| `0x44` `'D'` | `CMD_READ_FLASH_RAW` | raw flash address ÷ 64 (§6) | response: 64 bytes |
| `0x41` `'A'` | `CMD_APP_ERASE` | **slot index 0–7** | none |
| `0x61` `'a'` | `CMD_APP_WRITE` | byte offset within the slot (§7) | exactly 512 bytes |
| `0x4C` `'L'` / `0x6C` `'l'` | read/write boot logo | byte offset into the 40,960-byte logo | 512-byte chunks |
| `0x53` `'S'` | `CMD_SAT_WRITE` | byte offset into the 5,120-byte satellite table | 512-byte chunks |
| `0x45` `'E'` | `CMD_END` | 0 (ignored) | none; radio ACKs then reboots |
| `0xEE` | `CMD_ERROR` | echoes request addr | 1-byte error code (§3) |

Only the first six rows are used by the web flasher's Step 4; logo/satellite
commands are documented for completeness.

## 5. The logical address space (CMD_READ / CMD_WRITE) — the address-mapping answer

`CMD_READ`/`CMD_WRITE` take a **16-bit logical address that is NOT a raw
flash address**. The radio dispatches on exact address values
(`src/cps/mod.rs:handle_read` / `handle_write`):

| Wire addr | Content | Read len | Write len | Notes |
|---|---|---|---|---|
| `0x0000`–`0x7FFF`, multiple of 32 | channel record # `addr / 32` (0–999) | 32 | 32 | |
| `0x8000` (`VFO_INFO_ADDR`) | VFO raw block | 64 | 64 | |
| **`0x9000` (`RADIO_IMFOS_ADDR`)** | **Settings record** | **168** | **168** | §8 |
| `0xA020` + 16·n (n = 0–19) | DTMF contact #n | 16 | 16 | base `DTMF_CODE_ADDR` |
| `0xC000` (`FM_ADDR`) | FM channel list (30 × u16 LE) | 60 | 60 | |

- Anything else — an unlisted address, a non-multiple offset, or a write
  whose length is not exactly the listed length — is refused with
  `ERR_BAD_ADDR (0x02)`. There are **no partial or offset reads/writes** in
  this space: one frame moves exactly one whole record.
- **How this maps to flash:** the numeric values deliberately mirror the
  low end of `flash_map::addr` (channels at flash `0x0000`, VFO region at
  `0x8000`, settings region at `0x9000`, FM at `0xC000`), because the
  storage layer's wear-leveled regions are based at those same flash
  addresses (`src/device/storage.rs`: `SETTINGS_REGION =
  WearLeveledRegion::new(addr::RADIO_IMFOS_ADDR, 16)`). But the CPS
  commands go **through the storage layer**, not to raw flash:
  - The Settings record physically lives in a **wear-leveled region at
    flash `0x9000`** (16-sector region; each stored copy is payload + 2
    sequence bytes behind a bitmap header — `src/hal/wear_leveled.rs`).
    Raw flash at `0x9000` is therefore **not** a plain copy of the record.
    Never read or write settings via `CMD_READ_FLASH_RAW`.
  - ⚠️ **Correction to earlier project notes:** the Settings record is
    *not* stored at `SYSTEMRAN_ADDR 0xE000`. `0xE000` hosts a different,
    8-byte wear-leveled record (per-side channel/VFO state,
    `CHANNEL_STATE_REGION`). The task brief's "settings at 0xE000" was
    wrong; the wire address `0x9000` is the only settings address a host
    ever uses.
  - App slots (flash `0x14C000…`) and the boot logo (flash `0xC0000`)
    **cannot be addressed at all** in this logical space (they exceed
    16 bits / have no dispatch entry) — they have their own commands
    (§6, §7).

### Settings writes are parsed, not copied

`handle_write` at `0x9000` runs the 168 received bytes through
`Settings::from_bytes` and stores `settings.to_bytes()`. Several fields are
**clamped on parse** (`src/flash_map.rs:from_bytes`): e.g. `key_auto_lock`
≤ 3, `vox_level` 1–9, `roger_tone` ≤ 2, `rtone` ≤ 3, `scan_mode` ≤ 2,
`save_level` ≤ 4, `backlight_time` ≤ 4, `channel_display_mode` ≤ 2,
`rptrl` ≤ 10, `vox_delay` ≤ 15, side-key functions ≤ 10,
`band_lock` ≤ 9, `boot_display_mode` ≤ 2, observer coords range-checked.
Reads return `load_settings().to_bytes()` — i.e. **what a read returns is
already the firmware-normalized serialization**, so a host read-modify-write
that changes one byte and writes the record back round-trips stably. The
byte offsets in §8 are offsets in this serialized record.

## 6. CMD_READ_FLASH_RAW — the verification path

`handle_read_flash_raw`: the response is the **64 raw flash bytes** at
`wire_addr × 64`, for any address in the 2 MiB flash chip
(`0x00000`–`0x1FFFFF` → wire `0x0000`–`0x7FFF`). This is the only CPS read
that bypasses the storage layer, and it is how a host verifies app pushes:

- Slot *s* header (108 bytes, §7) starts at flash
  `0x14C000 + s × 0x8000`; the first 64 bytes are at wire
  `(0x14C000 + s × 0x8000) / 64 = 0x5300 + s × 0x200`, the next 64 at +1.
- An erased slot reads back as all `0xFF` (bad magic = launcher "EMPTY").
- Do **not** use this to read the Settings record (§5: wear-leveled).

## 7. Overlay app packages and the push sequence

### 7.1 Package format (`abi/src/lib.rs:PackageHeader`, `tools/mkapp.py`)

A `.app` file is one package, at most **32,768 bytes** (one slot). All
integers little-endian. Header is **108 bytes** at offset 0:

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | magic — ASCII **`OVL1`** (`abi::MAGIC`) |
| 4 | 4 | `build_hash` u32 LE — §7.2 |
| 8 | 1 | `segment_count` (1–4) |
| 9 | 3 | padding (zero) |
| 12 | 16 | name, ASCII, NUL-padded |
| 28 | 4 × 20 | segment entries: `{offset, image_len, bss_len, entry_off, crc32}` u32 LE each; unused entries zero |

Segment images follow the header at their stated offsets (4-byte aligned).
Per-segment `crc32` is the standard zlib CRC-32 of the segment image.

### 7.2 BUILD_HASH — the ABI gate

`abi/build.rs` generates `BUILD_HASH` as the **standard CRC-32**
(zlib-compatible: init `0xFFFFFFFF`, reflected, final XOR) of the exact
bytes of `abi/src/lib.rs`, and `mkapp.py:build_hash` computes the identical
value host-side. The firmware refuses to run a package whose header hash ≠
its own (`src/app/overlay.rs:slot_valid` / `load_segment`: magic **and**
build_hash **and** a valid segment 0 are required for the launcher to list
a slot as anything but "EMPTY").

**For our flashed firmware build** (upstream @ e52b236, unmodified
`abi/src/lib.rs`): **`BUILD_HASH = 0xDEC616E9`** (computed 2026-11-24 via
`zlib.crc32(abi/src/lib.rs)` on the build box, and confirmed by `mkapp` /
`check_app` output for every app in `APPS.md`). The web page embeds this
constant and rejects any `.app` whose header hash differs. If the firmware
is ever rebuilt from a tree with a changed `abi/src/lib.rs`, this constant
must be recomputed — it is an ABI version, not a firmware version.

### 7.3 Push sequence (`cps_proto.push_app`, radio side `handle_app_erase`/`handle_app_write`)

1. Handshake (§2).
2. **`CMD_APP_ERASE`, addr = slot index (0–7)**, no data. The radio erases
   the slot's full 32 KiB (8 × 4 KiB sectors at flash
   `0x14C000 + slot × 0x8000`) and **remembers the slot for the rest of the
   session**. Response: empty-data ACK frame. An out-of-range slot gets
   `ERR_BAD_ADDR`. (Host reference waits with the normal 2 s frame timeout
   and retries up to 5× on timeout; re-erasing is harmless.)
3. **`CMD_APP_WRITE`, addr = byte offset within the slot** (0, 512, 1024,
   …), **data = exactly 512 bytes**, for each chunk of the package. The
   package is padded with **`0xFF`** to a whole number of 512-byte chunks
   (`payload.ljust(ceil(len/512)*512, b"\xff")`). The radio requires:
   a preceding erase in this session (else `ERR_BAD_ADDR`), offset a
   multiple of 512, length exactly 512, offset + 512 ≤ 32,768. Writes go to
   raw flash at `overlay_slot_addr(slot) + offset` via the storage layer.
   Each chunk is ACKed with an empty-data frame before the next is sent.
4. Verify (web-flasher addition, not in `push_app.py`): read the slot back
   with `CMD_READ_FLASH_RAW` (§6) — at minimum the 108-byte header (two
   64-byte reads) — and compare magic, build_hash, segment_count, name and
   segment entries against the file that was pushed; a full-package
   read-back compare is stronger and cheap (≤ 64 reads).
5. `CMD_END` (or just stop): the radio reboots; the launcher then lists the
   slot by the package's name.

Note the session-state trap: the erased slot is remembered **per session**.
If the session times out (2 s idle) mid-push, the radio reboots and the
host must re-handshake and **re-erase** before writing further chunks.

## 8. The Settings record and the band-lock byte (`src/flash_map.rs`)

168 bytes (`SETTINGS_BYTES`), read/written whole at wire addr `0x9000` (§5).
Full field map is `Settings::from_bytes` / `to_bytes`; the fields a host
needs for Step 4:

| Offset | Field | Encoding |
|---|---|---|
| 0 | `sql_level` | u8 |
| 3 | `tx_forbid` | u8 ≠ 0 = true — **a second, independent TX kill**: if set, PTT does nothing at all (`src/app/tx.rs`) |
| 21–22 | `battery_cal_raw` | u16 LE |
| **29** | **`band_lock`** | **u8, `BandLock` profile index, clamped ≤ 9 on parse** |
| 30 | `boot_display_mode` | u8 ≤ 2 |
| 40–55 | `boot_text_line1` | 16 bytes ASCII |
| 56–71 | `boot_text_line2` | 16 bytes ASCII |
| 72–167 | `boot_tune` | 48 × (tone, duration) byte pairs |

### BandLock profiles (`src/device/radio.rs:BandLock`)

| # | Label | TX allowed |
|---|---|---|
| 0 | CE HAM | 144–146 MHz, 430–440 MHz |
| 1 | FCC HAM | 144–148 MHz, 420–450 MHz |
| 2 | GB HAM | 144–148 MHz, 430–440 MHz |
| 3 | 400-430 | 137–174 MHz, 400–430 MHz |
| 4 | 400-438 | 137–174 MHz, 400–438 MHz |
| 5 | PMR446 | 446.00625–446.19375 MHz |
| **6** | **GMRS/FRS** | **462.550–462.725 MHz, 467.550–467.725 MHz, + MURS 151.820 / 151.880 / 151.940 / 154.570 / 154.600 MHz** |
| 7 | CA HAM | 144–148 MHz, 430–450 MHz |
| 8 | ALL LOCK | nothing — TX prohibited everywhere |
| 9 | UNLOCK | the hardware's full synthesizer range |

Parse rule: any stored byte > 9 is clamped to 9 by `from_bytes`, but
`BandLock::from_u8` maps only 1–9 explicitly and **everything else
(including 0) to CE**; the value the radio *displays/uses* is the clamped
stored byte. RX is never gated by band lock. Preston's radio is set to
**6 (GMRS/FRS)** — he is GMRS-licensed (WSFQ833); that profile covers every
GMRS/FRS channel.

### Band-lock write procedure (web flasher)

1. Handshake. 2. `CMD_READ @ 0x9000` → 168-byte record; note byte 29 (and
byte 3). 3. Copy the record, change **only byte 29** to the chosen profile.
4. `CMD_WRITE @ 0x9000` with the full 168 bytes → empty-data ACK (a `0xEE`
frame = failure, abort). 5. `CMD_READ @ 0x9000` again and **verify byte 29
== chosen value** from the read-back, not from the write ACK. 6. `CMD_END`;
radio reboots with the new lock in force.

## 9. Source disagreements and traps (flagged)

1. **Settings storage address (brief vs source):** the task brief said the
   settings record lives at flash `SYSTEMRAN_ADDR 0xE000`. Source says the
   settings wear-leveled region is based at `RADIO_IMFOS_ADDR 0x9000`
   (`src/device/storage.rs`); `0xE000` holds the 8-byte channel-state
   record. Firmware source wins; §5 documents the correct mapping. This
   error is harmless at the CPS layer because hosts never address settings
   storage directly — wire `0x9000` was always the right host address.
2. **`src/ui/cps.rs` is not the CPS implementation** — it only draws the
   "CPS / PROGRAMMING" screen (23 lines). The protocol lives in
   `src/cps/mod.rs` + `src/cps/frame.rs`.
3. **Unknown commands** get `ERR_BAD_LEN (0x03)`, not a dedicated
   unknown-command error (`cps/mod.rs` default arm) — hosts must not read
   `0x03` as necessarily a length problem.
4. **Write errors are all `ERR_BAD_ADDR (0x02)`** — `handle_write` collapses
   bad-address and bad-length into one code, unlike the frame layer's
   distinct `0x03`.
5. **`cps_proto.py` has no general read/write helpers** — only `send()` plus
   app/sat/logo pushers. The settings read/write in §8 is assembled from
   `send(CMD_READ/CMD_WRITE, 0x9000, …)` semantics and the radio-side
   dispatch; the mod's `cps_tool.py` confirms the same framing independently.
   Upstream and mod copies of `cps_proto.py` are byte-identical (no
   disagreement there).
6. **`push_app.py` never verifies** what it wrote. The §7.3 step-4
   read-back is a web-flasher addition built on `CMD_READ_FLASH_RAW`,
   which the radio implements but no upstream host tool uses for apps.
7. **Erase timeout:** `cps_proto` gives `CMD_APP_ERASE` the standard 2 s
   frame timeout (its 20 s `ERASE_TIMEOUT_S` is used only for logo/sat
   first-chunks). Erasing 32 KiB is radio-side work inside one frame
   exchange; the retry-on-timeout policy covers a slow erase, and a
   duplicate erase is idempotent.
