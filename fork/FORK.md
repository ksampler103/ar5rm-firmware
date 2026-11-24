# Fork 1 — AR-5RM standby themes, per-variant builds

**Status: BUILT AND CATALOGUED (2026-11-24), not yet flashed.**
Four firmware variants, one per standby theme, are in the web
flasher catalog (`docs/firmware/bf5rh-fork1-*.BF`). Flashing any of
them needs Preston's separate explicit go for that flash, as always.

## Architecture (reworked 2026-11-24, Preston's direction)

The first fork design compiled all themes into one firmware with a
runtime picker (THEMES overlay app writing a `THM1` block the core
read from flash). It measured **64,744 B against the 61,440 B limit**
and stopped at the size gate. Preston redirected: **audit/slim the
core, and choose themes in the web flasher instead** — one firmware
variant per theme, each carrying exactly ONE standby renderer;
switching themes on a radio = flashing a different variant ("select
a theme, it uploads the new theme and deletes the old").

- Theme selection is a **cargo feature**: `theme-apx` (default),
  `theme-clean`, `theme-dense`, `theme-minimal`. `src/ui/theme.rs`
  const-asserts exactly one is enabled. Build a variant with e.g.
  `cargo build --release --no-default-features --features theme-dense`.
- The standby dispatch in `src/ui/standby.rs` is cfg-gated; each
  variant's ELF was verified (symbol table) to contain exactly its
  own `<theme>_full` / `<theme>_dyn` pair and no other renderer's.
- **The THM1 contract is RETIRED.** Nothing reads or writes app-data
  sector **0x195000** anymore; it is free space again. The `App`
  theme field, the boot-time block read, and `refresh_theme()` are
  removed — `src/app/mod.rs` is byte-identical to upstream again.
- No THEMES app exists or is needed; the flasher catalog is the
  picker.

## Base and reproduction

- Upstream: bf-5rh firmware, commit **e52b236** (the tree our flashed
  M0 build came from; raw 57,492 B).
- Fork tree (build box): `~/ar5rm-firmware/bf5rh-fork`, commits
  **dae59c4** (first design) → **7d2de0d** (this rework) on e52b236.
  The pristine upstream tree is untouched.
- Recover from this repo alone:

  ```sh
  git clone <upstream bf-5rh> && cd bf-5rh-firmware-open
  git checkout e52b236
  git apply fork/bf5rh-fork.patch   # 73,051 B
  cargo build --release                                   # APX Pro
  cargo build --release --no-default-features --features theme-dense
  ```

- Delta vs upstream (4 files, nothing else):
  - `Cargo.toml` — the four `theme-*` features (default `theme-apx`).
  - `src/ui/theme.rs` (new) — theme ids, palettes, compile-time
    `ACTIVE_THEME` selection + the exactly-one-feature assertion.
  - `src/ui/standby.rs` — the four renderers behind cfg gates on a
    shared snapshot/cache core (see "Themes").
  - `src/ui/mod.rs` — registers the theme module; the `TextBuf`
    slimming refactor (see "Core audit").
- **The `abi` crate is untouched**, and `BUILD_HASH` is derived only
  from `abi/src/lib.rs` (see `abi/build.rs`). Verified two ways for
  this build: the generated `build_hash.rs` reads
  **0xDEC616E9**, and `tools/check_app.py` against the existing
  `fm.app` reports all OK (build_hash 0xDEC616E9, size, stack budget,
  CRC). All 11 catalog apps remain valid against every variant (the
  abi is feature-independent, so one proof covers all four).

## Core audit (upstream M0 ELF, 57,492 B)

Sections: `.vector_table` 212, `.text` 51,232, `.rodata` 5,968,
`.data` 80. Sized-symbol attribution (rust-nm, demangled; flash
unless noted; totals reconcile to the image within inlined /
anonymous-symbol slop):

| Subsystem | Bytes | Notes |
|---|---:|---|
| `app::overlay` | 15,565 | incl. **8,192 B ARENA in RAM** (.uninit) — flash cost ≈ 7,373 |
| `__cortex_m_rt_main` | 6,104 | `main()` with app logic inlined into it |
| `ui::draw` | 3,472 | UI dispatcher with screen draws inlined |
| `ui::standby` | 2,794 | upstream single layout |
| `app::App` methods | 2,690 | incl. `App::new` 1,208 |
| `cps` (protocol) | ≈ 2,398 | `run_session` 2,092 + framing/response helpers |
| `ui::list` | 1,944 | shared list widget (launcher, menus, scan) |
| `drivers::fd6818b` | 1,724 | RF transceiver driver |
| `app::tx` | 1,276 | mostly `set_ptt` |
| `device::radio` | 1,204 | |
| `device::storage` | 1,168 | wear-leveled settings/VFO storage |
| `ui::TextBuf` | 1,028 | 9 capacities monomorphized — **slimmed, see below** |
| `flash_map::Settings` | 960 | `from_bytes` 664 + `to_bytes` 296 |
| `hal::wear_leveled` | 928 | |
| `ui::boot` | 746 | boot message/logo screens |
| `drivers::rda5807` | 610 | FM broadcast tuner driver (core side of fm.app) |
| embedded-graphics (all) | ≈ 3,948 | `Text::draw` 1,452 (single instantiation), primitives ≈ 1,580, mono_font ≈ 550 |
| `core::fmt` | ≈ 1,800 | `Formatter` 806 + `write` 258 + integer `Display` impls ≈ 890 |
| `core::str` | ≈ 1,100 | utf8 convert/count/error paths |
| compiler_builtins `memcpy` | 682 | |

### Safe cuts — applied

| Cut | Saving | Evidence |
|---|---:|---|
| `TextBuf::write_str` / `as_str` delegate to shared non-generic cores (`textbuf_push` / `textbuf_as_str` in `src/ui/mod.rs`) | **−396 B** | theme-less base 57,492 → **57,096 B**, measured on a pristine-tree build with only this change; output behaviour identical by construction (same copy semantics, one body) |
| THM1 runtime machinery removed (block parse + CRC-8 + boot/refresh reads) | (in fork delta) | APX variant lands at 57,092 B — the removal plus const-folded palette offsets the APX additions (focus border, status-bar clock) against the slimmed base |

### Experiments — rejected with measurements (theme-less base)

| Experiment | Result |
|---|---:|
| Linker `--icf=safe` | 0 B saved; links clean, vector table identical — no foldable duplicates. Not adopted (risk without benefit). |
| `-C target-cpu=cortex-m4` | **+4,216 B** (61,312) — worse. Rejected. |
| `debug = false` in release profile | 0 B on the raw image (debug info never enters the .bin). Kept `debug = true` — it costs no flash and aids symbol work. |
| `opt-level = "s"` | Link fails: overflows FLASH by ≈ 3.4 KB. `opt-level = "z"` stands. |
| Drop an embedded font | 0 B — all three fonts are referenced outside standby (proven in the first fork run). |
| Float-format diet | 0 B available — no float formatting or double-precision runtime exists in the image (verified in the symbol table). |

### Feature-cut menu — NOT applied (Preston's call, estimates ±10%)

Nothing below was cut. Each is listed with what it would break, in
case a future fork needs the room (no current variant does — see the
size table; the tightest margin is 4,280 B).

| Candidate | Est. bytes | What breaks |
|---|---:|---|
| Overlay app system (`app::overlay` flash portion) | ≈ 7,373 | **Every overlay app** — settings/fm/spectrum/chanmgr/satellite/search/scanqt + all 5 custom apps. The single biggest item and the worst trade on the list. |
| CPS protocol (`cps::run_session` + helpers) | ≈ 2,398 | All web-flasher CPS functions: settings read/write, band-lock changes, app install/update. |
| `core::fmt` pad diet (hand-roll the 16 padded-integer format sites: `{:02}`, `{:05}`, …) | ≈ 738 | Nothing user-facing **if** every replacement is byte-exact — but freq/timer/channel formatting across standby, list, scan, boot and CPS all depend on it. Classified as a diet, not a safe cut: needs an output-diff verification pass before it earns "safe". |
| Built-in scan mode (`app::scan` + `ui::scan` + scan list plumbing) | ≈ 630+ | The core's own band-scan screen. Partially duplicated by the SPECTRUM and SEARCH overlay apps, which is why it's on the menu at all. |
| FM tuner core glue (`drivers::rda5807` + overlay `api_fm_*`) | ≈ 610+ | fm.app stops working (no broadcast radio). |
| Boot melody player (boot_tune sequencing) | ≈ 200–400 (not separately attributable) | The programmable power-on melody (a settings/CPS feature) — boot text and logo screens stay. |
| Satellite support in core | ≈ 0 | Nothing to cut: pass prediction lives in the SATELLITE app; the core only stores observer lat/lon inside the settings record. |
| compiler_builtins `memcpy` diet | up to 682 | Requires nightly `build-std` feature flags; not available on the stable toolchain in use. |

## Themes

| Variant | Feature | Renderer |
|---|---|---|
| APX Pro | `theme-apx` (default) | Upstream standby geometry, palette-threaded, plus UTC clock in the status bar and the amber focus border on the master band. |
| Clean Modern | `theme-clean` | Status pill, master/slave cards, per-card signal text, MIC bar. Square corners (rounded corners cost ~1.4 KB of embedded-graphics code). |
| Data Dense | `theme-dense` | All-FONT_5X8 data sheet: A box (name/SQL, RX+offset, TX+tone, power/bandwidth/RSSI), B section, last-signal and status lines, live header (clock, battery volts). |
| Minimal Dark | `theme-minimal` | Rebuilt for this rework from the approved mockup (the first run's version was dropped before its only commit and was not recoverable): battery icon top-left, dim clock top-right, one huge centred frequency, channel name under it, hairline, other band as one dim line. No meter/flags; the frequency carries RX/TX state by colour (white idle, green RX, amber last-signal, red TX). Input states (freq/channel/DTMF entry) render centred in place of the frequency. |

All variants keep the upstream standby behaviours: dual-watch rows,
RX/TX/last-signal indicators, S-meter/MIC meter (except Minimal,
by design), frequency / channel / DTMF input states, and the
TX PROHIBITED / NO CHANNELS overlays.

Palette-izing the shared screens (boot message, list/launcher) is
still not done — the palettes exist in `theme.rs`, but the shared
screens keep upstream colours in every variant. `scan.rs` and the
CPS screen keep upstream colours regardless (documented scope-out).

Deviations from the mockups (unchanged from the first run): UTC-only
clock (no timezone support in the firmware), battery shown as volts
+ icon (no % source exists), square Clean Modern cards, Data Dense
bottom lines show last-signal/status instead of the mockup's
app-data line.

**Pre-flash theme verification exists** (2026-11-24): the core-UI
simulator (`tools/simulator/`, `gen_core.py`) compiles these exact
renderer sources for the host and renders every variant from
Preston's real channels/settings — frames in
`tools/simulator/examples/standby-*.png`, and the web flasher's
picker previews are these renders, not the mockups. Two deltas the
renders surfaced that the mockup comparison had not documented:
the Clean Modern palette as shipped is a saturated royal blue
where the mockup is a muted slate, and the `·` separators written
in `standby.rs` (5 places) render as `?` — the embedded-graphics
ASCII fonts have no middle-dot glyph, so the radio will show the
same `?`. Both are cosmetic; neither is fixed here (any fix is a
firmware change and bumps the version).

## Variant sizes (raw .bin; limit 61,440 B)

| Variant | Raw size | Margin | .BF size | .BF SHA-256 |
|---|---:|---:|---:|---|
| Fork 1 — APX Pro | 57,092 | 4,348 | 57,108 | `3edf67505ce2c6d44202f715266755c1526614626101462f2f125158d87a63ad` |
| Fork 1 — Clean Modern | 57,160 | 4,280 | 57,176 | `cf329090449b556bb836b4e1de9bc8b1390817116317711a49e991f21e73f13a` |
| Fork 1 — Data Dense | 56,920 | 4,520 | 56,936 | `5cf38268cacf777462013b6d290ff0a174f23c996b2c7b8cec42bba749fede60` |
| Fork 1 — Minimal Dark | 54,284 | 7,156 | 54,300 | `bed070996edf47138e495d00b586dd2efbc096b4d2f4568c43ab61dc77b60f54` |

All four fit with room to spare; no variant needed the feature-cut
menu. (For reference: slimmed theme-less base 57,096 B; upstream M0
57,492 B.) The .BF wrap recipe was verified byte-for-byte against
the known-good M0 package before wrapping (empty data file, same
tool — the re-wrapped M0 reproduces the flashed file's SHA-256
`b04356cb…` exactly).

## On-radio verification list (for Preston, when a flash is approved)

1. Flash **Fork 1 — Data Dense** (`bf5rh-fork1-dense.BF`, SHA-256
   `5cf38268…` above) through the flasher's usual gates.
2. Standby should match the core-UI simulator's Data Dense render
   (`tools/simulator/examples/standby-dense.png`): full data sheet
   for the A band (name/SQL, RX freq + offset, TX freq + tone,
   power / bandwidth / live RSSI), B section below, clock + battery
   volts in the header.
3. Behaviour checks: dual-watch rows both live; RX indicator + RSSI
   move on a NOAA channel; TX on GMRS shows the TX state and timer;
   the NO CHANNELS / TX PROHIBITED overlays still appear when their
   conditions are forced.
4. Installed apps still launch from the launcher (settings, fm,
   spectrum, chanmgr — build hash unchanged, they should just work).
5. Web CPS still connects (Phase 1 functions: settings read,
   band-lock write with read-back).
6. **Theme swap proof:** flash **Fork 1 — APX Pro**
   (`bf5rh-fork1-apx.BF`, SHA-256 `3edf6750…`) the same way — standby
   is now the APX Pro layout. Power-cycle persistence is inherent:
   the theme is compiled in, there is nothing to lose or re-set.
7. Channels, settings and installed apps should be untouched by the
   swap (same flash layout, same settings record, same app slots).

## Changelog

- 2026-11-24 (evening) — Core-UI simulator renders all four
  variants from the shipping sources (fork commit unchanged,
  7d2de0d); flasher previews swapped from mockups to the true
  renders. No firmware change, no version bump.
- 2026-11-24 (later) — Rework per Preston's redirect: core audit +
  TextBuf slimming (−396 B theme-less base), compile-time per-theme
  variants replace the runtime THM1 design (contract retired, sector
  0x195000 free again), Minimal Dark rebuilt from the mockup, four
  variants wrapped and catalogued. Fork commit 7d2de0d.
- 2026-11-24 — Fork created from e52b236; theme storage contract
  fixed at 0x195000; APX Pro / Clean Modern / Data Dense implemented,
  Minimal Dark implemented then dropped on size; size engineering
  pass (−2,744 B from the naive build); stopped at 64,744 B
  (+3,304 over) per the size-gate protocol. Superseded by the rework
  above the same day.
