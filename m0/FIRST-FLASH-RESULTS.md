# First flash — results (2026-11-24)

Package: bf5rh-m0.BF (57,508 B, SHA-256 b04356cb…c90357e), unmodified upstream
bf-5rh build. Flashed to the NRF dev unit (5RH10W-V05) via the web flasher on
Windows after cable backup (factory_memory.img, 33,664 B, SHA-256 4dffae06…,
archived outside the repo) and a successful bootloader probe. All 57 packages
ACKed; radio rebooted into bf-5rh. Preston's explicit go given 02:41 EST.

## Works (Preston's bench report, 2026-11-24)
- Boots; display looks great (ST7735 assumption correct — the teardown's #1
  unknown is resolved).
- RX works: NOAA weather channel received at -42 dBm (FD6818 driver, 26 MHz
  reference, RX path all confirmed on the V05 board).
- Up/down tuning works; keypad works.
- Stock channels carried over: bf-5rh uses the stock flash layout
  (CHAN_ADDR 0x0000), so the unit's saved channels were already in its format.

## Explained non-faults (root-caused in upstream source)
- MENU shows "EMPTY" for every row: MENU opens the overlay-app launcher
  (src/app/launcher.rs -> overlay::slot_name). Slots with no valid app
  package print "EMPTY" (src/app/overlay.rs:190). This build has no overlay
  apps installed. 8 slots at flash 0x14C000. Upstream ships apps/ sources +
  tools/mkapp.py + tools/push_app.py; the jakegarrett mod ships prebuilt
  .app packages (fm.app, settings.app, spectrum.app, ...). ABI/BUILD_HASH
  compatibility with our build must be checked (tools/check_app.py) before
  pushing any.
- "TX PROHIBITED" on every frequency tried: BandLock (src/device/radio.rs).
  settings.band_lock is read from the settings record in external flash
  (SYSTEMRAN 0xE000, byte 29 of the 168-byte record) and gates TX in software
  before the PA keys (src/app/tx.rs -> radio.enter_tx). Profiles: 0 CE HAM
  (144-146/430-440), 1 FCC HAM (144-148/420-450), 6 GMRS/FRS (462.55-462.725,
  467.55-467.725, MURS), 8 ALL LOCK (TX nowhere), 9 UNLOCK (hardware ranges).
  RX is never gated. Fix path: write band_lock via the bf-5rh CPS protocol
  (radio now speaks PROGRAMBF-5RH) — no reflash needed. Preston is
  GMRS-licensed (WSFQ833); recommended profile 6.
- FM broadcast mode unreachable: in this firmware generation FM is an
  overlay app, not a built-in mode (src/app/keyfn.rs: "FM radio was made as
  app"; key function 5 is unmapped; FM tune/seek exposed only via the
  overlay API in src/app/overlay.rs). Same root cause as EMPTY menu —
  installing fm.app restores it. The 5807M hardware path is therefore still
  untested, not failed.

## Still to test
- TX itself (after band_lock fix): GMRS simplex 462.5625, stock BK unit
  listening.
- Settings persistence across power cycle; volume/beep/backlight/LEDs.
- FM after fm.app is installed.

## Next work item
Extend the web flasher with bf-5rh CPS support (PROGRAMBF-5RH transport):
(1) read/write the settings record's band_lock byte; (2) push overlay app
packages into slots. Both ride the same CPS framing (mod's cps_proto.py is
the reference implementation).

## Phase 1 CPS session — results (2026-11-24, Windows, web flasher Step 4)
First live CPS exchange against real hardware: full success, log archived
from Preston's session (03:05–03:08 EST).
- Settings read: band_lock byte 29 = **0 (CE HAM)** — the record matches
  Settings::DEFAULT (e.g. battery_cal_raw 2537), i.e. bf-5rh wrote first-boot
  defaults; the stock firmware's settings were not carried into this record.
  That fully explains "TX PROHIBITED" on GMRS (CE allows only 144–146 /
  430–440). TX Forbid byte 3 = 0.
- Band lock changed **0 → 9 (UNLOCK)** at Preston's direction (the page
  recommended GMRS/FRS 6; he chose UNLOCK in the confirm step). Write ACKed,
  verified by read-back; a later independent read confirmed byte 29 = 9.
- Apps installed and verified by full read-back byte-compare:
  settings.app -> slot 0 (20,480 B padded), fm.app -> slot 1 (4,096 B padded).
  Final slot map read: slot 0 SETTINGS (3 segments), slot 1 FM, slots 2–7
  EMPTY, build hash matches on both.
- Preston's bench verdict: "Both apps work great" — FM app launches (5807M
  path exercised via the app) and Settings app runs. Remaining bench items:
  GMRS TX test with the stock BK unit listening, FM audio quality report.
- TX CONFIRMED Nov 24 (Preston, after the UNLOCK change): TX and RX both
  work great on the dev unit. First-flash catalog COMPLETE — every major
  subsystem on the 5RH10W-V05 verified under bf-5rh: display, keypad,
  receiver, transmitter/PA, FM broadcast tuner, external-flash storage,
  CPS, overlay apps.
