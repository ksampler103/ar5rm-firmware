# Changelog

Firmware versions for the AR-5RM custom firmware (bf-5rh fork). Scheme:
see `VERSIONING.md`. Newest first. All dates 2026-11-24 unless noted.

## v1.0.0 — Fork 1: slimmed core + theme variants

First release of our fork of upstream bf-5rh (base commit e52b236).

- **Core slimming (safe cuts only):** the theme-less base went from
  57,492 B (upstream M0) to **57,096 B** (−396 B). The one real duplication
  found by a symbol-level audit — `TextBuf`'s `write_str`/`as_str`
  monomorphized at 9 capacities — now delegates to shared non-generic
  cores. No behavior change, no feature removed. Larger candidate cuts
  (overlay system ≈7,373 B, CPS ≈2,398 B, built-in scan ≈630 B, FM core
  glue ≈610 B, boot melody ≈200–400 B) are documented unapplied in
  `fork/FORK.md` as a priced menu.
- **Themes as per-firmware variants:** the standby screen theme is chosen
  in the web flasher by flashing the variant that carries it (switching
  themes = flashing a different variant). Four variants, each the slimmed
  core plus exactly one compiled-in standby renderer:
  - **APX Pro** — upstream geometry, palette-threaded, master-band focus
    border. Raw 57,092 B.
  - **Clean Modern** — card layout, per-card signal text, mic bar.
    Raw 57,160 B.
  - **Data Dense** — full 5×8 data sheet with live RSSI fields and a
    last-signal line. Raw 56,920 B.
  - **Minimal Dark** — one huge frequency, almost nothing else.
    Raw 54,284 B.
  All four fit the 61,440 B app limit with 4.3–7.2 KB of margin. (An
  earlier all-themes-in-one build reached 64,744 B and was rejected at
  the size gate; the per-variant design replaced it, and the planned
  on-device theme-picker app and its THM1 flash-block contract were
  retired before shipping.)
- **Compatibility:** the `abi` crate is untouched; BUILD_HASH is unchanged
  at **0xDEC616E9**, verified from the build output and by `check_app.py`.
  All 11 catalog overlay apps remain valid on every variant.
- **Status:** built and verified by construction/check_app; **not yet
  flashed** to hardware as of this release.

Assets (flashable packages):

| File | Size | SHA-256 |
|---|---|---|
| bf5rh-fork1-apx.BF | 57,108 B | 3edf67505ce2c6d44202f715266755c1526614626101462f2f125158d87a63ad |
| bf5rh-fork1-clean.BF | 57,176 B | cf329090449b556bb836b4e1de9bc8b1390817116317711a49e991f21e73f13a |
| bf5rh-fork1-dense.BF | 56,936 B | 5cf38268cacf777462013b6d290ff0a174f23c996b2c7b8cec42bba749fede60 |
| bf5rh-fork1-minimal.BF | 54,300 B | bed070996edf47138e495d00b586dd2efbc096b4d2f4568c43ab61dc77b60f54 |

## v0.1.0 — M0 baseline (upstream bf-5rh)

The unmodified upstream bf-5rh build (raw 57,492 B) that started the
project — the port-feasibility milestone ("M0").

- **First flash 2026-11-24** to the NRF dev unit (board 5RH10W-V05) via
  the web flasher, after a full stock backup (archived outside this repo)
  and a successful bootloader probe. All 57 packages ACKed; the radio
  rebooted into bf-5rh.
- **Verified on hardware:** display (ST7735), keypad, up/down tuning,
  receiver (NOAA WX at −42 dBm), TX/PA (after the BandLock setting was
  corrected — the radio had inherited the CE HAM profile; set to UNLOCK
  via CPS), FM broadcast tuner (via the FM overlay app), external-flash
  storage, CPS, and overlay apps. Stock channels carried over intact
  (bf-5rh uses the stock flash layout).
- Establishes the baseline every fork change is measured against, and the
  BUILD_HASH (0xDEC616E9) that all catalog apps are built against.

Asset (flashable package):

| File | Size | SHA-256 |
|---|---|---|
| bf5rh-m0.BF | 57,508 B | b04356cbeebb91a1aa0b593e5e1414df1fc1a3cc6fae716b1f0c36875c90357e |

## Apps & tools (no firmware version bump)

Noted here per `VERSIONING.md`; these ship through the web flasher
catalog, not firmware releases.

- 2026-11-24: Web flasher redesigned — progressive disclosure (Step 0
  only until connected), automatic stock/custom/flash-mode detection,
  enforced stock flow (Backup → Probe → Flash), custom hub
  (Upgrade / CPS / Apps), graphical app picker with descriptions, usage
  notes and simulator screenshots. Protocol code byte-identical to the
  proven build.
- 2026-11-24: Catalog reached 11 overlay apps (Settings, FM, Spectrum,
  ChanMgr, Satellite, Search, ScanQT, plus customs ACTLOG, RPTBOOK,
  FOXHUNT, WXPANEL) and a host-side app simulator.
