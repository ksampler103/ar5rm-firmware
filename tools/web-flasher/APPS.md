# Overlay apps built for our bf-5rh firmware

Built 2026-11-24 on clt-build-01 from the **same upstream source tree as the
flashed firmware** (`bf-5rh-firmware-open` @ `e52b236`), using the repo's own
flow: `tools/mkapp.py` to pack, `tools/check_app.py` to validate. Because the
apps and the firmware share one `abi/src/lib.rs`, their `BUILD_HASH` matches
the firmware's by construction.

- Firmware ABI `BUILD_HASH`: **`0xDEC616E9`** (CRC-32 of `abi/src/lib.rs`)
- Package magic: `OVL1`; header 108 bytes; slot size 32,768 bytes (all fit)
- Suggested slots follow upstream `tools/install_all.py`'s default layout.

| File | App name | Segments | Size | SHA-256 | check_app verdict | Suggested slot |
|---|---|---|---|---|---|---|
| `apps/fm.app` | FM | 1 (image 3,692 B, bss 4 B) | 3,800 B | `54913aec9fdd575d5bd88faeb5b24581bb57cf02054f1b3d3955afd4d622c2e3` | PASS — build_hash, size, stack budget, crc all OK | 1 |
| `apps/settings.app` | SETTINGS | 3 — menu (6,392 B), radio (6,752 B), system (6,812 B); segment order menu=0, radio=1, system=2 per `apps/settings/common.rs` | 20,064 B | `56241c25308bfa09abd49fd28b706551cf8fb85d2510324e5bd560d7d3a469a7` | PASS — all checks OK | 6 |
| `apps/spectrum.app` | SPECTRUM | 1 (image 6,092 B, bss 1,348 B) | 6,200 B | `ff8d85aa9f9fb36512925f59ba4a62e265723928f1a6e1031c66c9d0ea0d9063` | PASS — all checks OK | 7 |
| `apps/chanmgr.app` | CHANMGR | 1 (image 7,700 B, bss 136 B) | 7,808 B | `2460505575c2bf60c9e7543bd06f8238f1582dc32f00b919ebeee46dd2382f07` | PASS — all checks OK | 2 |
| `apps/satellite.app` | SATELLITE | 3 — list (6,028 B), predict (11,992 B, oversized), track (7,544 B); segment order list=0, predict=1, track=2 per `apps/satellite/common.rs` | 25,672 B | `e430affeca4525851629dbcbd0fd98600353cab2c80a4ebd0ab08e8b4769bd5e` | PASS — build_hash, size, stack budget (+924 B margin), crc all OK | 3 |
| `apps/scanqt.app` | SCANQT | 1 (image 928 B, bss 8 B) | 1,036 B | `9d101d4ea4b4554384c3f47dc2c71389db6b3d106eced45fe26d2e5bab6697bf` | PASS — all checks OK | 4 |
| `apps/search.app` | SEARCH | 1 (image 2,316 B, bss 24 B) | 2,424 B | `ea8cd069ca3c8c1816f1b29a797e77026329f1e74675e19d5854800a5d171e5d` | PASS — all checks OK | 5 |

Phase A build notes (2026-11-24, second run):

- The remaining three upstream apps (`satellite`, `search`, `scanqt`) were
  built with the identical flow. One gotcha: this box's `toolbin/` was
  missing `rust-objcopy` — symlinked from the rustup sysroot
  (`lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-objcopy`).
- `satellite` needs its satellite table in the app-data region
  (flash `0x18C000`, written by upstream `tools/push_sat.py`) and an
  observer position in settings before it can predict passes; the package
  alone launches to an empty list. Suggested slots above complete upstream
  `install_all.py`'s default layout (0 is reserved for hello, not built).

Build notes:

- `mkapp.py` needs `cargo` plus the llvm-tools `rust-objcopy` / `rust-nm` /
  `rust-size` / `rust-objdump` on PATH. On clt-build-01 these live in the
  rustup sysroot's `lib/rustlib/x86_64-unknown-linux-gnu/bin/` (with
  `llvm-nm`/`llvm-objdump`/`llvm-size` symlinked as `rust-*` in
  `~/ar5rm-firmware/toolbin/`). First failure mode if missing: bare
  `FileNotFoundError: 'cargo'` / `'rust-objcopy'` from `mkapp.py`.
- The `settings` app is a 3-segment package; its crates must be passed to
  `mkapp.py` in the order `menu radio system` (segment indices are
  compile-time constants in `apps/settings/common.rs`).
- Validation used `check_app.py <file>.app` (header checks: build_hash,
  size, stack budget, per-segment CRC). The origin/relocation checks only
  run when packing from crate dirs and were not repeated — the packages
  came straight out of `mkapp.py` in the same tree.

## Custom apps (ours, 2026-11-24)

Sources live in `custom-apps/<name>/` (standalone cargo projects in the
upstream app skeleton; built on clt-build-01 from a scratch copy inside
the upstream tree so the `../../abi` path dep resolves — upstream sources
were not modified). All four carry build hash `0xDEC616E9` and PASS
`check_app.py`.

| File | App name | Segments | Size | SHA-256 | check_app verdict |
|---|---|---|---|---|---|
| `apps/actlog.app` | ACTLOG | 1 (image 10,080 B, oversized) | 10,188 B | `45ea08934ebe353d81cb01ae2f72cc646fad92acf3e8bc45518528512b25cafb` | PASS — stack budget +3,092 B margin |
| `apps/rptbook.app` | RPTBOOK | 1 (image 10,160 B, oversized) | 10,268 B | `a058997f5b01d12ed81aff9ae13bab8797b547f99d488fd9c0f9e196ed5884d8` | PASS — stack budget +2,996 B margin |
| `apps/foxhunt.app` | FOXHUNT | 1 (image 4,656 B) | 4,764 B | `cc75095c292b8ce7d9925820b4fd0e969d0e8f705fcf766835ea3cb37ae78d0b` | PASS — all checks OK |
| `apps/wxpanel.app` | WXPANEL | 1 (image 5,632 B) | 5,740 B | `70f52eb5c0a4d82596d94fc204f2b209bf4730cdd7aa10a7668b994aeb90eb96` | PASS — all checks OK |

ACTLOG and RPTBOOK are *oversized* segments (image > ARENA_SIZE 8,192 B,
within ARENA_MAX 12,288 B) — the same mechanism upstream's satellite
predict segment uses; their `app.x` declares LENGTH = 12288.

### ACTLOG — activity logger

Scans the radio's programmed channels (ABI `chan_read`, deduped by
frequency; fallback fixed list of NOAA WX + GMRS outputs + 146.520 /
446.000 when no channels exist) at ~90 ms worst-case dwell, tone-agnostic
carrier detect, speaker muted while running. A hit = squelch open
≥ 200 ms, closed by 300 ms of silence; records frequency, start time,
peak RSSI, duration.

**Persistence: YES — survives app exit and power cycles.** ABI evidence:
the firmware fences `nor_write`/`nor_erase_sector` to addresses
≥ `OVERLAY_APP_ADDR` (0x14C000) and `flash_map.rs` designates everything
past the slots (`OVERLAY_DATA_ADDR` 0x18C000) as app-private data —
the satellite app already persists this way. ACTLOG's region is
0x191000..0x195000 (clear of satellite's 0x18C000..0x191000): header
sector at 0x191000 (magic `ALOG`, version), then an append-only log of
16-byte records across 3 sectors — **768 hits max**. The end of the log
is the first all-0xFF record, so appends never need an erase. Review
view (MENU) shows the newest 48 hits; `*` + MENU clears (the only erase
path besides first-run init). Times are UTC when the radio clock has
been set (`utc_get`), otherwise hits show duration/peak only. Known
limits: dwell means very short transmissions can be missed; the clock
is unset after power-on until something sets it, so timestamps depend
on that.

### RPTBOOK — repeater book (with injected-data contract)

Directory of **Preston's real parsed channels: 165 entries** generated
by `custom-apps/rptbook/gen_baked.py` from his pre-flash backup
(`factory_memory.img`, bf-5rh `flash_map::Channel` layout: BCD deci-Hz
frequencies, 12-byte names, firmware subaudio tone codes) into
`src/baked.rs`. Read-only — the app never writes channel data. MENU
tunes the entry in-app (exact RX/TX/tones); `#` shows a detail view;
on EXIT after a tune, `save_master_vfo` lands the standby VFO on the
entry's RX frequency (with the TX tone if any — the standby VFO applies
its own offset rules after exit; the in-app tune is the exact one).

**Injected data block (ZIP-code flow, Preston's design amendment):**
the app supports a data block patched into its flash slot at install
time, so a future page flow can do ZIP → nearby repeaters → compose
slot image → existing install, with **no app rebuild per location**.

- *Mechanism:* real slot read via the ABI — `nor_read` has **no lower
  bound** in the firmware (only writes are fenced), so at startup the
  app scans the 8 slot headers (base 0x14C000, stride 0x8000) for the
  package named `RPTBOOK` and reads the block at `slot_base + 0x6000`.
  No file patching of the .app binary is needed.
- *Offset:* **0x6000** (24,576) from the slot base. The package
  (10,268 B) must stay below 0x6000 for the contract to hold.
- *Format:* 16-byte header — magic `RPBK`, u8 version = 1, u8 reserved,
  u16 record count, u16 **CRC-16/XMODEM** (poly 0x1021, init 0) over
  the record bytes, 6 bytes reserved — then fixed **30-byte records**:
  name 16 B ASCII (space/NUL padded), rx_hz u32 LE, tx_hz u32 LE
  (0 = same as rx), tx_tone u16, rx_tone u16 (both in the firmware's
  own subaudio encoding: 0 = none, 1–105 = DCS normal table index,
  106–250 = DCS inverted, >250 = CTCSS tenths of Hz), flags u8
  (bit0 duplex, bit1 tx tone set, bit2 rx tone set, bit3 RX-only),
  1 byte reserved.
- *Capacity:* (32,768 − 24,576 − 16) / 30 = **272 records max**.
- The app validates magic, version, count range, and CRC; any failure
  → the baked table is used. Full contract also documented in the
  header comment of `custom-apps/rptbook/src/main.rs`.

### FOXHUNT — direction-finding helper

Live RSSI on the master frequency: numeric dBm (firmware conversion,
raw − 170), bar graph with decaying peak-hold marker, guidance line
(STRONG/WARM/WEAK + body-block/attenuation prompts), MENU snapshots a
bearing sample (up to 8, timestamped when the clock is set), `#`
toggles a beeper whose rate follows RSSI (tone_on is in the ABI, so
the beeper is real, ~120 ms–1.5 s interval over −45…−128 dBm),
`*` clears samples/peak, UP/DOWN step ±12.5 kHz.

### WXPANEL — NOAA panel

The 7 WX channels (162.400–162.550) in a list with per-channel RSSI;
on entry it samples all 7 (3 reads each after a 60 ms settle) and
auto-tunes the strongest, then keeps a live RSSI readout on the tuned
channel. UP/DOWN + MENU tune manually, `*` re-runs the auto-scan.

## Commit decision

The four `.app` files **are committed** in this directory (`apps/`). They
are our own build outputs from GPL-3.0 upstream source, each is only a few
KB, and committing them means GitHub Pages (or any static host) can serve
them alongside the flasher page later — Preston's stated direction for this
tooling. They are **not** stock firmware and contain no radio data, so the
project rule against committing stock images does not apply. Rebuild them
from source rather than trusting the committed copies if the firmware's
`abi/src/lib.rs` ever changes — the web flasher rejects any package whose
build hash doesn't match the firmware it was built for.
