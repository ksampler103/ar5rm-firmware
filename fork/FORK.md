# Fork 1 — AR-5RM standby theme system

**Status: BLOCKED ON FLASH SIZE (2026-11-24).** The theme system is
implemented and compiles, but the raw binary is **64,744 B** against the
**61,440 B** application limit (over by **3,304 B**), so the fork tree
does not link with the stock 60K `memory.x` and **no .BF has been
produced, wrapped, catalogued, or flashed**. See "Size report" below
for the numbers and the remaining cut options. Nothing in this fork
has been near a radio.

## Base and reproduction

- Upstream: bf-5rh firmware, commit **e52b236** (the same tree our
  flashed M0 build came from; raw 57,492 B).
- Fork tree (build box): `~/ar5rm-firmware/bf5rh-fork`, fork commit
  **dae59c4** on top of e52b236.
- Recover from this repo alone:

  ```sh
  git clone <upstream bf-5rh> && cd bf-5rh-firmware-open
  git checkout e52b236
  git apply fork/bf5rh-fork.patch   # 66,216 B, from this directory
  cargo build --release             # compiles; link overflows 60K FLASH
  ```

- Delta vs upstream (4 files, nothing else):
  - `src/ui/theme.rs` (new) — theme ids, palettes, THM1 block parsing.
  - `src/ui/standby.rs` — theme dispatch + APX/Clean/Dense renderers.
  - `src/ui/mod.rs` — registers the theme module; re-reads the theme
    when the UI enters standby.
  - `src/app/mod.rs` — `App::theme()` / `refresh_theme()`; theme read
    from flash at boot.
- **The `abi` crate is untouched**, and `BUILD_HASH` is derived only
  from `abi/src/lib.rs` (see `abi/build.rs`). Verified in the fork
  build's generated output: **BUILD_HASH = 0xDEC616E9**, identical to
  upstream, so all 11 existing catalog apps remain valid against this
  firmware by construction. (`check_app.py` against the fork was not
  run end-to-end because the firmware does not link; the hash evidence
  above is stronger — it is the exact value the apps are checked
  against.)

## Theme storage contract

Apps cannot write core flash (NOR writes are fenced to ≥ 0x14C000);
the core can read anywhere. The theme therefore lives in the app-data
region:

- **Sector: `0x195000`** — the first free 4K sector after the existing
  allocations (overlay apps end at 0x18C000; satellite table
  0x18C000–0x191000; actlog data 0x191000–0x195000; flash chip ends at
  0x200000, so 0x195000 collides with nothing and leaves the rest of
  the app-data region free).
- **Block format:** byte 0–3 magic `"THM1"`, byte 4 = theme_id,
  byte 5 = CRC-8 (poly 0x07, init 0x00) over bytes [0..5], rest 0xFF.
- Core reads the block at boot and again whenever the UI enters
  standby (so a theme written by the THEMES app applies on return to
  standby, no reboot needed). Missing / bad magic / bad CRC /
  out-of-range id → theme 0.

## Themes

| ID | Name | Status |
|----|------|--------|
| 0 | APX Pro | Implemented — upstream standby geometry, palette-threaded, plus UTC clock in the status bar, focus border on the master band. Default/fallback. |
| 1 | Clean Modern | Implemented — status pill, master/slave cards, per-card signal text, MIC bar. Square corners (rounded corners cost ~1.4 KB of embedded-graphics code and were cut; see below). |
| 2 | Data Dense | Implemented — all-FONT_5X8 data sheet: A box (name/SQL, RX+offset, TX+tone, power/bandwidth/RSSI), B section, last-signal and status lines, live header (clock, battery volts). |
| 3 | Minimal Dark | **Reserved, not built in.** Palette and id exist; the id currently renders as APX Pro. Dropped under the design rule "include only if genuinely cheap" — it is not (see size report). |

All implemented themes keep the upstream standby behaviours: dual
watch rows, RX/TX/last-signal indicators, S-meter/MIC meter, frequency
/ channel / DTMF input states, and the TX PROHIBITED / NO CHANNELS
overlays.

Palette-izing the shared screens (boot message, list/launcher) was
planned but **not done** — it is downstream of a firmware that fits,
and would only add code. `scan.rs` and the CPS screen keep upstream
colours regardless (documented scope-out in the design).

## Size report (the blocker)

Raw binary sizes (objcopy -O binary of the release ELF; limit 61,440):

| Build | Raw size | vs limit |
|-------|----------|----------|
| Upstream M0 (flashed, verified on radio) | 57,492 B | −3,948 |
| Fork, 4 themes (first full build) | 67,756 B | +6,316 |
| Fork, 4 themes, after size engineering | 66,016 B | +4,576 |
| Fork, 3 themes (Minimal dropped) — **current** | **64,744 B** | **+3,304** |

Attribution (llvm-nm, fork vs upstream): `.text` +8,080 B, `.rodata`
+536 B. The `ui::standby` module grows from ~2.8 KB of symbols to
~11.4 KB; each additional full layout with incremental redraw costs
roughly **2.2–2.5 KB** of code (Clean ≈ 2,528 B, Dense ≈ 2,224 B,
Minimal ≈ 1,140 B, APX ≈ upstream parity + ~300 B). The three mockup
layouts are geometrically distinct (rows vs cards vs dense text
sheet), so the hoped-for single parameterized renderer did not
materialize — each theme is its own full+dynamic renderer pair
sharing only helpers (freq/tone/offset formatting, signal reading,
battery/status/meter state).

Cuts already made and documented (in order applied):

1. Rounded corners on Clean Modern cards → plain rectangles.
   embedded-graphics' `RoundedRectangle` cost **1,430 B** on its own.
2. `#[inline(never)]` sweep over layout entry points and shared
   helpers, stopping LTO from cloning helper bodies per call site.
3. Format helpers converted to `&mut dyn core::fmt::Write`, collapsing
   one generic instantiation per TextBuf size into a single copy;
   shared `fmt_timer` / `fmt_volts` / `signal_reading` helpers.
4. Minimal Dark dropped (pre-authorized by the design when not cheap).

Prescribed thinning steps that yield **zero** here, verified:

- (a) *Drop the largest unused font:* no font is droppable. All three
  firmware fonts are referenced outside standby (FONT_9X18 by
  `scan.rs` via `draw_frequency`, FONT_5X8 by `list.rs`, FONT_6X10
  everywhere), so removing one from standby does not unlink it.
- (b) *Integer-format hot paths replacing float fmt:* the standby code
  contains no float formatting at all (all integer already).

### What's left to cut (pick one direction)

Estimates from measured per-function symbol sizes, ±10%:

- **Drop Data Dense too** (−~2,270 B) → ~62,474 B. Still ~1,034 over;
  needs the micro-cuts below as well.
- **Drop Clean Modern too** (−~2,530 B) → ~62,214 B. Still ~1,034 over;
  same caveat.
- **APX + Minimal only** (restore Minimal, drop Clean and Dense)
  → ~61,084 B est. **Fits**, but keeps the bonus theme and loses two
  of the three approved ones — backwards from Preston's ranking
  (Data Dense was the favourite).
- **APX + one approved theme + micro-cuts** (~600–900 B available:
  single-computation dyn-cache refactor, Dense write!-push rework,
  Clean info-line via shared helpers) → borderline ~61,500 B; high
  effort, some risk, no simulator exists for the core UI to verify
  against (the app simulator does not cover core standby).
- **Shrink upstream features** (CPS session 2,092 B, overlay system
  6,692 B, …) — out of scope for this fork; would regress the
  verified M0 feature set.
- **Revisit the 60K region** — not possible: the AT32F421C8 has
  64 KB flash total, 4 KB is the stock bootloader, 61,440 B is a
  physical limit, not a linker preference. (An `--icf=all` linker
  experiment was tried and rejected: it folded the cortex-m entry
  symbol and produced a broken ELF.)

## Not done (downstream of a fitting firmware)

- THEMES overlay app (picker writing the THM1 block) — not built.
- Simulator proof of the THEMES app — not run (nothing to prove yet).
- Boot/list palette pass — not done (see above).
- Wrapping (.BF), flasher catalog, APPS.md — not done.
- Flashing — **not done and not authorized**; needs Preston's separate
  explicit go for that specific flash, as always.

## Changelog

- 2026-11-24 — Fork created from e52b236; theme storage contract
  fixed at 0x195000; APX Pro / Clean Modern / Data Dense implemented,
  Minimal Dark implemented then dropped on size; size engineering
  pass (−2,744 B from the naive build); stopped at 64,744 B
  (+3,304 over) per the size-gate protocol, with the cut menu above.
