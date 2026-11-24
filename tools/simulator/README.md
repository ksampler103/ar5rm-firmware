# bf-5rh overlay-app host simulator

Run bf-5rh overlay apps on a PC — no radio required. The harness
implements the firmware's `bf5rh_abi::Api` vtable against mock state
seeded with Preston's real data (his 165 channels and his settings
record from the pre-flash backup), rasterises drawing calls into a
160×128 RGB565 framebuffer with the firmware's own fonts and list
widget, and drives apps with scripted key/tick/transmission scenarios,
writing PNG frames.

Its first jobs were validating the custom apps (actlog, rptbook,
foxhunt, wxpanel) off-radio and proving the rptbook **RPBK injected
data block** end-to-end — the mechanism the ZIP-code repeater-book
page will use — before any of that touches hardware.

## Quick start

```sh
# 1. Generate host wrappers for the apps (copies sources; upstream is
#    never modified). Paths default to the build box layout; override
#    with BF5RH_UPSTREAM / BF5RH_CUSTOM_APPS if yours differ.
python3 gen_wrappers.py

# 2. Build everything (13 apps).
cargo build --workspace

# 3. Run a scenario; frames land in runs/<app>/.
target/debug/sim-rptbook scenarios/rptbook.txt --out runs/rptbook

# 4. The injected-data proof: build an RPBK block, place the real
#    rptbook.app package + block into mock flash slot 2, run.
python3 tools/make_rpbk.py rpbk-demo.bin \
    "SIM ALPHA" 146520000 146520000 0 0 \
    "SIM BRAVO" 446000000 441000000 1413 0
target/debug/sim-rptbook scenarios/rptbook-injected.txt \
    --out runs/rptbook-injected \
    --slot-app 2:../web-flasher/apps/rptbook.app \
    --slot-data 2:0x6000:rpbk-demo.bin
```

Example frames are in `examples/`.

## How it works

- **Apps are compiled for the host.** Upstream apps are `#![no_std]`
  binary crates whose only firmware-provided glue is a panic handler
  and `#[no_mangle]` on `app_entry`. `gen_wrappers.py` copies each app
  into `gensrcs/` (gitignored), strips exactly those items, and emits
  one wrapper lib crate **per segment** — cargo features are per-crate
  and multi-segment apps (settings) specialise a shared `common.rs`
  per segment via features, which are transplanted from the segment's
  own manifest. A tiny generated bin then calls
  `bf5rh_sim::main_for(name, &[segment entries])`. The harness
  supplies the process panic behaviour; an app panic surfaces as a
  Rust panic naming the app code location.
- **The ABI is vendored byte-identically** (`abi/`, copied from
  upstream), so `BUILD_HASH` — a CRC32 of `abi/src/lib.rs` — still
  matches the firmware and the shipped `.app` files.
- **Firmware semantics are mirrored**, not approximated, where apps
  can observe them: draw callbacks are no-ops outside Draw dispatch;
  the framebuffer is cleared to black before the first Draw after
  Enter/Chain; `draw_list` uses the firmware's own `ui/list.rs` and
  its list-cache behaviour; NOR writes are fenced to ≥ 0x14C000 with
  AND (1→0) semantics and 4K-sector erase; the handoff buffer survives
  `Chain` (max 4 hops) while app statics do too (see Limitations);
  settings accessors parse the real 168-byte record; subaudio tables
  and formatting are copied from the firmware.
- **Mock state** (`harness/src/state.rs`, `rf.rs`): channels and
  settings load from `data/` (extracted from Preston's backup and the
  CPS session log — second read, band_lock = 9). The RF model gives
  per-frequency levels: the 7 NOAA channels strong with **162.475 MHz
  at −42 dBm** (his bench measurement), a few GMRS signals, Charlotte
  FM stations; everything else sits at the −135 dBm floor. Override
  with `--rf file` (`<hz> <dbm>` lines; see `scenarios/quiet.rf`).
  The 2 MiB mock NOR flash can be loaded/saved with
  `--flash`/`--flash-out`, which is how actlog's persistence across
  runs is demonstrated.
- **Clock**: scenario steps advance a virtual clock in 10 ms ticks;
  the uptime API additionally folds in real elapsed time so apps that
  busy-wait on uptime (spectrum's dwell loops) terminate as they do
  on hardware. The RF model and event scheduling stay on the pure
  scenario clock for determinism.

## Scenario format

One step per line (`#` comments):

```
utc 816940800        # set UTC (seconds since 2000-01-01); default is
                     # 2025-11-24 08:00 UTC
assertutc 816940800  # fail the run unless the harness UTC is within
                     # ±2 s of the given value (proves utc_set calls)
wait 500             # advance 500 ms (10 ms ticks, Draw after each)
key DOWN             # key event: 0-9 * # MENU EXIT UP DOWN VM AB BAND
                     # SIDE1 SIDE2, optional kind press|release|single|
                     # long|repeat (default single)
tx 462587500 -65 20000   # a transmission: hz, dBm, duration ms
shot list-top        # write <name>.png (160x128) and <name>@4x.png
note text            # print a marker into the run log
```

CLI flags: `--out DIR`, `--data DIR`, `--rf FILE`, `--flash FILE`,
`--flash-out FILE`, `--slot-app SLOT:FILE` (place a real `.app`
package into mock flash), `--slot-data SLOT:0xOFFSET:FILE`.

## API coverage

Every `Api` callback is implemented. Two honesty tiers:

**Full firmware semantics** — all drawing (`fill_rect`, `blit`,
`draw_text` in the three embedded-graphics fonts, `draw_line`,
`draw_circle`, `draw_list`), `handoff_read/write`, `settings_read/
preview/commit`, `side_cfg_read/write`, `settings_get` (sql/tot/vox/
beep/observer coords from the real record), `chan_read/chan_write`,
`nor_read/write/erase_sector` (fencing + AND semantics), the FM
receiver set (`fm_tune_khz`, `fm_status`, `fm_channels_load/save`,
`fm_power_off`, …), subaudio index/code/format (verbatim tables),
`enter_rx/retune_rx` and the `set_*` radio state, `tx_state`,
`set_tx_enabled`, `utc_get/set`, `fmt_u32/fmt_freq/freq_to_str`,
`save_master_vfo`, `uptime_100us`, `delay_ms`. `app_fault` and
`factory_reset` end the process loudly (exit codes 70/71), like the
firmware leaving the app.

**Modelled** (first use prints a `[stub]` note in the run log; the
run summary lists every callback the app actually used):
`read_rssi` and `squelch_open` answer from the RF model (squelch
threshold = the model's −117 dBm base, ±3 dB per sql level from 3,
with sql seeded from his real settings); `fm_seek` jumps instantly to the next modelled station;
`freq_scan`'s check reports the strongest modelled signal near the
tuned frequency; `detect_subaudio` answers from an optional
per-frequency tone in the RF config, else "none"; `battery_raw12_avg`
is fixed at 2500 (≈ his radio's calibration region);
`read_rf_gains` returns fixed plausible gains; `read_mic_level` is 0;
`settings_get(7)` chip id is 0; `correct_measured_freq_word` is the
identity. Nothing returns silent zeros without a note.

## Per-app verdicts (2026-11-24)

| app | result |
|---|---|
| hello | ✅ spike app; text/rect/circle/line all render |
| rptbook | ✅ baked list shows Preston's real channels; detail view works; **injected RPBK block replaces the directory** (title shows `RPTBOOK (ZIP)`) |
| wxpanel | ✅ samples all 7 WX channels, auto-selects 162.475 MHz at −42 dBm |
| timesync | ✅ injected TSYN block (built by `tools/make_tsyn.py`) sets the harness clock to the payload — `assertutc` proves it and the frame shows 2026-11-24 16:42:31 UTC; with no block the clock is untouched and the guidance frame shows |
| actlog | ✅ scans his channels, logged a simulated hit (462.5875, −65 dB, 6 s), review screen works, log **persists across processes** via the flash image |
| fm | ✅ main screen, stepping, seek lands on the next modelled station |
| settings | ✅ group list → Chain into the radio segment shows his real values (SQL 3, TOT 8) |
| chanmgr | ✅ lists his real channels |
| scanqt | ✅ main screen (standby = his channel 1, 136.100) |
| spectrum | ✅ sweeps and renders; needed the real-time uptime fold-in |
| search | ✅ hunting screen renders |
| foxhunt | ✅ quiet screen, and a −50 dBm burst shows full-scale "VERY STRONG" |
| satellite | ✅ list segment renders its (correct) empty state — no satellite table or observer position exists in Preston's data, exactly as on his radio; predict/track segments compile but are unreachable without a table, in sim as on hardware |

## Adding an app

Add its name/layout to `APPS` in `gen_wrappers.py` (single `.` segment
or a list of segment directories), re-run the generator, and write a
scenario. Custom apps are read from the repo
(`../web-flasher/custom-apps` by default), upstream apps from
`BF5RH_UPSTREAM`.

## Core-UI simulator (fork standby screens)

The app simulator above runs overlay apps; the fork's **core
standby screen** is rendered by a sibling mode, `gen_core.py`. It
copies the fork's actual renderer sources (`src/ui/standby.rs`,
`theme.rs`, `icons.rs` — verbatim, provenance in `coresrcs/`) into
one generated crate per theme variant under `gencore/`, because the
theme is a compile-time cargo feature of the fork, exactly as in
the firmware. Each crate pairs the renderer with a mock
`crate::app` (`coreui/mock_app.rs`, hand-written and checked in)
that implements exactly the getters the renderer calls, with the
firmware's own semantics for the derived values (S-meter curves,
`rssi = raw − 170`, battery volts formula — copied from
`src/app/mod.rs`).

```sh
python3 gen_core.py            # BF5RH_FORK overrides the fork path
cargo build -p bf5rh-coreui-apx -p bf5rh-coreui-clean \
            -p bf5rh-coreui-dense -p bf5rh-coreui-minimal
./target/debug/sim-standby-dense                 # idle frame
./target/debug/sim-standby-dense --rx            # squelch open on master
```

State mapping (defaults = the design mockups' scenario, from
Preston's real data): master = his channel #21 "GMRS 20"
(462.6750), slave = #43 "WX3PA4" (162.4750), loaded from
`data/channels.bin` by the firmware's own `load_channel` rules;
settings (SQL, dual watch, channel display mode, battery
calibration) from `data/settings.bin` at the firmware's offsets.
The RF model supplies the slave's −42 dBm (his bench measurement);
`--rx` puts −71 dBm on the master and makes it the watched side.
The clock is scenario time (`--utc-mins`, default 232 = 03:52).
Battery bars use a **mock** calibration table — the radio's real
DEV_BATT table was never captured — chosen to read 3 of 4 bars at
the scenario voltage (7.90 V from his real `battery_cal_raw`).

Frames land in `runs/standby-<theme>/`; the verification set is
committed under `examples/standby-*.png` (plus `standby-dense-rx`).

Known limits of the core-UI mode:

- Only the standby screen: boot, list/launcher, scan and CPS
  screens are not rendered (they keep upstream colours in every
  variant anyway).
- One static state per run — no key input, no blink phase sweep;
  the frame is the renderer's second (dynamic) pass over a fixed
  snapshot.
- PNG colours are exact RGB565 expansions; the radio's ST7735
  panel shifts them (gamma, viewing angle), so treat hue as
  indicative, geometry and text as exact.
- States the mock cannot reproduce: anything needing the real
  battery calibration table (bar thresholds), a live microphone
  (MIC meter), or the satellite app's UTC fix (clock source).

## Limitations

- One app run per process; app `static`s therefore persist across
  Chain hops within a run (on hardware the arena is reloaded per
  segment — chained segments that pass state only via statics could
  differ; the shipped multi-segment apps use the handoff buffer,
  which is modelled exactly).
- Core UI beyond the standby screen is **not** simulated (see the
  core-UI section above for what is). What an app hands back on
  Exit (e.g. rptbook's tune handoff) is logged, not acted on by a
  standby screen.
- RF/FM/battery behaviour is a model, not a receiver: no fading,
  no adjacent-channel effects, squelch is a hard threshold.
- `delay_ms` advances the virtual clock without delivering ticks.
- Frames are the app's drawing only; the firmware's status bar /
  standby decorations are not composited.
