# CHIRP download-sequence diff — Baofeng 5RM / 5RH family

Researched 2026-11-24 from CHIRP's published source, after the web flasher's
Backup got **zero bytes back after the ident** on Preston's NRF-variant AR-5RM
(hold-8 `5RHNRF / Ver v0.14`), while CHIRP on the same Mac, same cable, same
radio read the radio successfully with model **"5RM"** selected (confirmed by
Preston — that is the mapping used throughout this diff).

**Headline: the wire protocol is identical.** Every byte CHIRP sends and
expects — ident, ACK, the three magics, the block-read frames, the memory
regions, the obfuscation — matches `PROTOCOL.md` §2 exactly. There is no
secret handshake we are missing. All differences are in the *envelope*:
retries, timeouts, pacing sleeps, and the serial-port layer underneath
(pyserial/termios vs Chrome Web Serial). The ranked list in §3 is therefore
about which envelope difference can produce "radio never enters program
mode / zero bytes after ident".

## 1. Sources

| Source | What was read |
|---|---|
| `chirp/drivers/baofeng_uv17Pro.py`, current (multi-ident / BLE-era revision, chirpmyradio attachment 14423, crawled 2026-01-17) | `_do_ident`, `_download`, `_sendmagic`, `_crypt`, class `UV17Pro`, class `BF5RM`, class `UV5RH`, sibling classes |
| `chirp/drivers/baofeng_uv17Pro.py`, older (pre-refactor revision, chirpmyradio attachment 10116 — the shape shipped in early-2025 CHIRP builds; debug logs from CHIRP next-20250117/20250221 show this `_do_ident`/`_sendmagic` line structure) | `_do_ident`, `_download`, class `UV17Pro` |
| `chirp/drivers/baofeng_common.py` (chirpmyradio attachment 7713) | `_rawsend`, `_rawrecv`, `_clean_buffer`, `STIMEOUT` |
| CHIRP debug logs (chirpmyradio attachments 12347, 12583) | Port-open parameters as actually logged by `chirp.wxui.clone` |
| CHIRP Supported Radios list | "Baofeng AR-5RM *(use 5RM)*" |

Note on a red herring: `baofeng_common.py` contains its own legacy
`_do_ident`/`_download` (9600 baud, a `\x02` byte, an `\xAA…\xDD` ident
string, `S`-framed reads). **The uv17Pro driver does not use them** — it
defines its own `_do_ident`/`_download` and binds them via
`download_function`/`upload_function`; from `baofeng_common` it only takes
`_rawsend`, `_rawrecv`, and `_clean_buffer`. Do not chase the 9600-baud path.

## 2. Step-by-step: CHIRP vs our spec

### 2.1 Model mapping (task item 7)

- CHIRP model **"5RM"** = class `BF5RM(UV17Pro)` in `baofeng_uv17Pro.py`
  (`VENDOR = "Baofeng"`, `MODEL = "5RM"`). CHIRP's supported-radios list maps
  "Baofeng AR-5RM" → "use 5RM". Preston confirmed he selected **5RM** and the
  read worked.
- `BF5RM` overrides **nothing** protocol-relevant: no `_idents`, `_magics`,
  `MEM_STARTS/SIZES`, or `_encrsym` of its own. It inherits the entire
  sequence from `UV17Pro`. (It only changes bands, power levels, lists.)
- Class `UV5RH` (MODEL `"UV-5RH"`) likewise inherits the same idents/magics;
  CHIRP issue #10889 notes the 5RM selection is the one to use for a UV-5RH.
- **There is no NRF/FD6818-specific class, ident, or fingerprinting step in
  CHIRP.** The FD6818 variant is served by the same `BF5RM` class and the
  same ident as the BK variant. Variant-specific idents exist only for other
  models in the same file: GM-5RH → `PROGRAMBFGMRS05U`, UV-17ProGPS →
  `PROGRAMCOLORPROU`, BF-F8HP-PRO → `PROGRAMBF5RTECHU`, K6 →
  `PROGRAMBF-K6PROU`. The current driver iterates a per-class `_idents`
  list; `BF5RM`'s list has exactly one entry (`PROGRAMBFNORMALU`).
- The only "fingerprint" is the 1-byte ACK check (`_fingerprint = b"\x06"`,
  response must *start with* it). `model_match()` (image bytes after
  MEM_TOTAL == MODEL string) is an image-file tag appended by `_download`
  itself, not something read from the radio.

### 2.2 Serial settings (task item 1)

| Aspect | CHIRP | Our spec / page | Differ? |
|---|---|---|---|
| Baud / format | 115200 8N1 (`BAUD_RATE = 115200`; older class attr `BAUDRATE`) | 115200 8N1 | No |
| Flow control | Off (logged at open: `xonxoff=False, rtscts=False, dsrdtr=False`) | Off | No |
| DTR / RTS manipulation in driver code | **None.** No `setDTR`/`setRTS` anywhere in `baofeng_uv17Pro.py` or `baofeng_common.py` (searched). | Page asserts both via `setSignals` (added in the DTR/RTS fix) | See below |
| DTR / RTS actual state | CHIRP debug logs record the opened port as **`(rts=True dtr=True)`** — pyserial asserts both lines by default at open and nothing ever lowers them | Asserted at Connect and again at Backup in the fixed build | Equivalent on paper |
| Port-open timeout | `timeout=0.25` at open (wxui clone), then raised before ident (below) | n/a (stream reader) | — |
| Read timeout during ident | **Current: 5 s** — `_download` sets `radio.pipe.timeout = 5` and the driver module sets `bfc.STIMEOUT = 5`. **Older: 1.5 s** — `_do_ident` sets `radio.pipe.timeout = STIMEOUT` (module-level 1.5) | ~1.5 s first-byte wait (page audit note), 0.25 s inter-byte after | **Yes — current CHIRP waits up to 5 s for the ACK** |
| Buffer flush before ident | `_clean_buffer()`: sets timeout to 0.005 s, reads up to 256 bytes, restores timeout — called before *every* ident attempt | Pre-ident listen + discard | Equivalent intent |
| Sleeps before ident | Current: none before attempt 1; 0.5 s before attempt 2, 1.0 s before attempt 3. Older: none at all | 300 ms settle after asserting signals (fixed build) | Minor |

### 2.3 Ident exchange (task item 2)

| Aspect | CHIRP | Our spec / page | Differ? |
|---|---|---|---|
| Bytes sent | `PROGRAMBFNORMALU` (16 bytes, `MSTRING_UV17L`) — identical | Same | **No** |
| Response read | Exactly `len(_fingerprint)` = **1 byte**; must start with `0x06` | 1 byte, must be `0x06` | No |
| Extra bytes around ident | None — no host ACK-back, no prefix/suffix byte, in either vintage | None | No |
| **Retries** | **Current: up to 3 attempts per ident**, each preceded by `_clean_buffer()`, with 0.5 s / 1.0 s backoff; a wrong/short ACK also triggers a retry. **Older: single attempt** | **Single attempt**, then Backup aborts | **Yes (current CHIRP)** |
| Post-send delay before reading | Current: `time.sleep(0.05)` inside `_sendmagic` after every send. Older: none | None (write then read immediately) | Minor |

### 2.4 Magic exchanges after ident (task item 3)

Identical in both CHIRP vintages and in our spec — verified byte-for-byte
from the current `_magics` list:

| # | Send | Read back |
|---|---|---|
| 1 | `0x46` | 16 bytes |
| 2 | `0x4D` | 15 bytes |
| 3 | `53 45 4E 44 21 05 0D 01 01 01 04 11 08 05 0D 0D 01 11 0F 09 12 09 10 04 00` (`SEND!…`, 25 bytes) | 1 byte |

Responses are not validated by CHIRP either (only length-checked by
`_rawrecv`). Differences are pacing only, and only in current CHIRP:
`time.sleep(0.15)` before each magic, and a `time.sleep(0.5)` "settle" after
the last magic before block reads. The older vintage sends the three magics
back-to-back with no sleeps, exactly like our spec. **None of this can
explain zero bytes after the ident** — all of it happens after a successful
ACK.

### 2.5 Block reads (task item 4)

Identical. CHIRP `_make_read_frame(addr, length)` =
`0x52 ‖ addr as u16 BE (struct.pack(">i", addr)[2:]) ‖ length as 1 byte`,
with `BLOCK_SIZE = 0x40`, i.e. `52 <hi> <lo> 40` — the same 4-byte request.
Response read is `BLOCK_SIZE + 4` = **68 bytes** (4-byte echo header + 64
data bytes); `_crypt` is applied to `d[4:]`. No per-block host ACK during
download (the `_ack_block`/per-block `0x06` traffic in `baofeng_common`
belongs to the legacy protocol and to *upload* in the old common code, not
to this driver's download). No retries on short reads — `_rawrecv` raises if
it gets fewer bytes than requested. Current CHIRP adds a 0.05 s sleep after
each frame send before reading; older does not. Our spec matches the older
pacing.

### 2.6 Memory regions and total size (task item 5)

Identical. `UV17Pro` (inherited unchanged by `BF5RM`):

- `MEM_STARTS = [0x0000, 0x9000, 0xA000, 0xD000]`
- `MEM_SIZES  = [0x8040, 0x0040, 0x02C0, 0x0040]`
- `MEM_TOTAL  = 0x8380` = **33,664 bytes** — the same four regions, same
  order, same total as `PROTOCOL.md` §2.2.

### 2.7 Crypt / obfuscation (task item 6)

Identical. CHIRP `_crypt(symbol_index, buffer)` with `_encrsym = 1` selects
key table entry 1 = **`CO 7`** (`43 4F 20 37`). Per byte: XOR with
`key[i % 4]` only if the key byte is not `0x20` (space) and the data byte is
not `0x00`, not `0xFF`, not equal to the key byte, and not equal to
`key ^ 0xFF`; otherwise the byte passes through unchanged. The index cycles
mod 4 over the buffer passed in, and `_download` passes exactly one 64-byte
block at a time — i.e. the key phase restarts per block, exactly as
`PROTOCOL.md` §2.3 describes. It is the same involution.

## 3. Ranked candidate causes of "zero bytes after ident"

The protocol bytes are proven identical (§2), and the same Mac + cable +
radio provably work under CHIRP. What remains, most likely first:

1. **Ident retries (current CHIRP) vs single-shot (our page).** Current
   CHIRP sends the ident up to 3 times, cleaning the input buffer before
   each attempt, with 0.5 s/1.0 s backoff. If the first ident is lost —
   garbled during adapter/radio UART lock-up, sent before the radio's CPS
   listener is ready, or eaten by a port-open transient — CHIRP recovers
   invisibly and the user just sees "CHIRP works". Our page sends once and
   aborts. *Caveat:* Preston manually retried Backup several times and got
   zero every time, so a purely random first-frame loss is a weaker fit
   than a systematic per-attempt difference (see #2/#4) — but each manual
   retry also re-ran the same page sequence, so a systematic defect would
   reproduce identically every time, which is also what we observed.
2. **ACK wait: 5 s (current CHIRP) vs ~1 s observed (our page).** Current
   CHIRP gives the radio up to 5 s to answer the ident (pipe timeout 5 s,
   `bfc.STIMEOUT = 5`). Preston's failing log shows the page declaring
   failure ~1 s after the ident was sent. If this NRF firmware is slow to
   switch into program mode on the first ident (the radio visibly reacted /
   TXed during page attempts, per Preston), a late ACK would be missed by
   the page and caught by CHIRP. This pairs naturally with #1: CHIRP's
   attempt 2 or 3 lands after the radio has woken up.
3. **Port-open → ident gap and line-state history.** Under CHIRP/pyserial
   the port is opened once (DTR/RTS asserted by the OS at open, then never
   touched), and human/UI time passes before the ident: clone-thread
   startup, `_clean_buffer`, and — per the driver's own `pre_download`
   prompt — the operator has typically just power-cycled the radio with
   the cable already connected ("1 - Turn off your radio, 2 - Connect your
   interface cable, 3 - Turn on your radio, 4 - Do the download"). A freshly
   booted radio listening from t=0, plus a port whose lines have been
   stable for seconds, is a different starting state from a page that
   asserted signals and sent the ident 300 ms later against a radio that
   had been sitting on (and had been through flash-mode button attempts).
4. **OS / driver layer: Chrome Web Serial vs pyserial on macOS with a
   Prolific clone.** The cable enumerates as "USB-Serial Controller D"
   (`cu.usbserial-1120`) — a PL2303-class clone on macOS's built-in driver.
   CHIRP reaches it through pyserial/termios; the page reaches it through
   Chrome's serial stack, which manages DTR/RTS and buffering differently
   (including line transitions at open/close and reader-lock behaviour).
   CHIRP working proves radio + cable + Mac driver; it does not prove
   Chrome's path delivers the ident the same way — and the one hard data
   point from the page logs is that the ident bytes left the page while
   literally nothing, not even an echo or garbage, ever came back.
5. **Pacing sleeps (current CHIRP only): 0.05 s post-send, 0.15 s between
   magics, 0.5 s post-ident settle.** Listed for completeness — these all
   act *after* bytes are already flowing (or after the ACK), so they
   cannot explain silence in reply to the ident itself. The older CHIRP
   vintage has none of them and also reads these radios.

### Suggested next experiments (implied by the ranking)

- Mirror current CHIRP's ident phase in the page: clean buffer → send
  ident → wait up to **5 s** for the 1-byte ACK → on silence, repeat for
  **3 attempts** with 0.5 s / 1.0 s backoff. This directly tests #1 + #2
  with no protocol change.
- Follow CHIRP's operating procedure literally for one attempt: radio off,
  cable seated, radio on, then Backup within a few seconds (#3).
- If both still yield zero bytes, the remaining suspect is the Chrome /
  PL2303-clone path itself (#4) — test the same page with a known-good
  FTDI/CH340 cable, or the same cable from a native (non-browser) run of
  the mod's `radio_read.py` on the Mac, to split browser-stack from cable.

## 4. Bottom line

CHIRP does not know any bytes we don't. For the confirmed "5RM" selection,
its download sequence is byte-for-byte the sequence in `PROTOCOL.md` §2 —
same ident, same ACK, same magics, same read frames, same regions, same
33,664 bytes, same "CO 7" crypt. The working difference is that current
CHIRP is *patient and persistent* at the ident step (3 tries, 5 s timeout,
buffer cleaned between tries) on top of a native serial stack, while our
page asks once, briefly, through Chrome's. Fix the envelope before
suspecting the protocol.
