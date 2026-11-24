# Board-identification checklist — check BEFORE any flash

Purpose: positively identify which hardware variant a UV-5RM/5RH-family unit is
before any firmware is written to it. The family ships at least three RF
variants (BK4819, AT1846S, FD6818) that are externally near-identical, and
firmware for the wrong variant boots (or not) with dead RF. Work top to bottom;
stop at the first mismatch.

Sources: project brief/teardown (`../PROJECT-BRIEF.md`,
`../teardown/TEARDOWN-2026-11-24.md`), amoxu RE repo README + `factory_firmware/`
(cloned 2026-11-24, `cc03870`), bf-5rh upstream README + `tools/flash.py`,
jakegarrett mod `FLASHING.md` (all on clt-build-01 under `~/ar5rm-firmware/`).

## 1. Hold-8 version screen (no tools, do first)

Power on while holding **8**. Record the string verbatim.

| String | Meaning | Implication |
|---|---|---|
| `5rhbk ver v0.14 bf` | BK-line unit (Preston's Unit 1) | RE README's BK4819 identification also includes a yellow round "BK" label under the battery. **Stays stock — never flash.** Its actual transceiver was never read; do not assume. |
| `5RHNRF / Ver v0.14 / bf` | NRF unit (Preston's Unit 2, the dev unit) | FD6818 variant per teardown of this exact unit (see §3). The jakegarrett mod explicitly lists `...NRF` units as don't-flash for *its BK-lineage build expectations* — read as: its author's gate, not a statement about this board. Our port targets this variant. |
| `5RHNRFB-YP07` | The mod's supported unit | Only string the shipped jakegarrett mod supports. Neither of Preston's units shows this. |
| v0.06 / v0.07 (any prefix) | Per amoxu README, likely AT1846S hardware, possibly different board design | Out of scope; do not flash with anything from this project. |
| Anything else | Unknown | Stop. Photograph the screen and the board before proceeding. |

Firmware ≥ v0.09 is the RE project's stated floor for its BK4819 work; both of
Preston's units are v0.14.

## 2. Board silkscreen (case open)

| Silkscreen | Line | Notes |
|---|---|---|
| `5RB10W-V01` | BK4819 line | amoxu's documented board. |
| `5RB10W-V02` | BK4819 line (presumed) | Preston's BK unit; one rev past V01, transceiver unread. |
| `5RH10W-V05` (+ `2350.`, date `23.10.12`) | **FD6818 line** | Preston's NRF dev unit. Different board code, own revision line. This project's target board — see `BOARD-5RH10W-V05.md`. |

## 3. Chip markings (case open, photograph everything)

- **RF transceiver:** `FD6818 / CS3382XA` + 26.0 MHz crystal beside it → FD6818
  variant (dev board, photo-verified). BK4819-line boards carry a Beken BK4819
  in the same role (amoxu hardware table). House-marked `HTL7G06S011P` QFNs in
  the antenna-end section are **PA modules (2×, one per band), not the
  transceiver** — do not misread them as the RF chip (this mistake was made and
  corrected on the BK unit).
- **SPI flash:** `25QH16CJIG` → XMC 25QH16, 16 Mbit (dev board confirmed;
  amoxu table lists the same part). Datasheet in the RE repo:
  `Datasheets/XM25QH16C_Ver1.7.pdf`. JEDEC ID: read at runtime once code runs
  (bf-5rh's `norflash.rs` does not hard-check an ID — it issues standard
  commands); **expected value to-confirm from the datasheet, not yet extracted.**
- **FM tuner:** `5807M` → RDA5807M family (dev board confirmed).
- **MCU:** LQFP-48; top marking illegible on both of Preston's units (red
  factory mark on the dev board). Identity rests on §4.

## 4. MCU identity over SWD (bench step, NOT part of M0)

- Expected: customized Artery **AT32F421C8T7** (silkprint SYN2A-000 per amoxu;
  Cortex-M4, 120 MHz, 64K flash / 16K SRAM, no Artery ISP bootloader — the
  system-bootloader region at 0x1FFFE400 holds config data instead).
- amoxu's README has a section titled "MCU part number detection over SWD" but
  the cloned README publishes **no expected IDCODE value** — so: **expected
  SWD IDCODE = to-confirm** at first bench session (record whatever the probe
  reports and check it against Artery's AT32F421 documentation before trusting
  it). Do not write an IDCODE into tooling from memory.
- Recovery context (brief): SWD wipe/reflash on this family demonstrated by
  porkfreezer's GM-5RH writeup with Artery's OpenOCD fork; SWD pads share pins
  with the RX/TX LEDs (PA13/PA14 in bf-5rh's `board/leds.rs` — consistent);
  reset must be held. SWD is a recovery path for *our* firmware only.

## 5. Register fingerprint tests (once our code first runs)

Cheap, decisive, run before trusting any RF behavior:

1. **FD6818 3-wire bus readback:** bf-5rh's driver (`src/drivers/fd6818b.rs`)
   talks to the chip over a bit-banged 3-wire bus (LE=PC13, SCK=PA2, SDIO=PA3,
   shared with the RDA5807 SDA line). Read a known register (e.g. the state
   word REG 0x30 / RSSI REG 0x67) after init: sane, changing RSSI on an open
   squelch = FD6818 answering. A BK4819 on the same pins would not follow the
   FD6818 register map (different register set/protocol family) — garbage or
   stuck values = wrong-variant board or wrong pins.
2. **Synthesizer lock check:** RadioFoundry's approach (`fd6818_detect` in
   `drivers/rf/fd6818/fd6818.c`) tries each setup variant at a probe frequency
   and checks the PLL-lock bit (REG 0x0C bit 9, `FD6818_CSS_PLL_LOCK`) within a
   timeout — default setup first, then setup B. Reuse this pattern to decide
   the setup-table question in `BOARD-5RH10W-V05.md`.
3. **Display:** bf-5rh assumes an ST7735-class 160×128 panel on SPI2
   (`src/drivers/st7735.rs`, MADCTL 0x60). A legible framebuffer after init is
   the fingerprint that the dev board's COG controller is ST7735-compatible;
   garbage/no image = controller hunt (the project's #1 unknown).

## 6. KDH flash mode — entry procedure (as documented upstream)

Two upstream sources document entry; they agree in substance, differ in detail:

- **jakegarrett mod `FLASHING.md` (tested procedure, most specific):**
  1. Radio **off**.
  2. Hold **both small side buttons next to PTT** (not PTT itself), keep holding.
  3. Power **on**. Screen stays **black** = flash mode. Release buttons.
  4. Verify non-destructively with the bootloader handshake
     (`bootloader_probe.py` in the mod; expect `BOOTLOADER ANSWERED`) before
     any write step.
- **bf-5rh upstream README:** "Hold the radio's side key while powering on so
  it enters flash mode" (singular "key", less specific).

Use the mod's two-button version as the working procedure; the discrepancy is
noted, not resolved — M0 made no radio contact to test either. The bootloader
protocol itself is implemented in bf-5rh's `tools/flash.py` (header 0xAA /
footer 0xEF, CRC16-XMODEM, handshake cmd 0x01, update cmds 0x03/0x04/0x05/0x45;
region 1 max 0xF000 = 60 KB enforced by the bootloader, region 2 max 0x1000).

Flashing mechanics (for completeness — **not authorized by M0**): wrap+encrypt
the raw binary with amoxu's `uv5rm-wrap-tool` into a `.BF`, then send it over
the K-connector serial port with `tools/flash.py`.

## 7. Mandatory pre-flash steps (any future flash, no exceptions)

1. **Config backup over the cable first.** Flash mode cannot read the radio.
   On a stock radio at its main screen, the mod's tooling reads a full factory
   memory image (`radio_read.py` → `factory_memory.img` per mod FLASHING.md
   step 1); bf-5rh also has a CPS protocol (`src/cps/`, handshake
   `PROGRAMBF-5RH`) once running our firmware. No backup, no flash.
2. Battery charged; cable fully seated (Baofeng jacks need a hard push).
3. Bootloader handshake probe (§6 step 4) passes before the write command.
4. Preston's separate explicit go for that specific flash (standing rule).

## 8. The one-way door — UPDATED by M0 findings

The brief recorded: *no v0.14 factory image exists for either variant, so
flashing is one-way off stock.* M0 found a candidate exception in the amoxu
repo: `factory_firmware/NRF_FD6818/5RH_NRF_Fangti_V0.14_231124.BF` (and a BK
v0.14: `BK_BK4819/5RH_BK_V0.14_231123.BF`).

Treat these as **unverified candidates, not a restore guarantee**:

- amoxu's README warns the factory files were collected from sellers/ham
  groups, provenance not guaranteed, and that his firmware files are known to
  work only on 5RB10W V01 hardware — the NRF folder's applicability to the
  5RH10W-V05 board is not stated anywhere in the repo.
- Filename/version match (`NRF`, `V0.14`, dated 231124) is encouraging but is
  not proof it matches Preston's unit's board revision or region config.
- Until a restore is actually demonstrated on the dev unit, **plan as if
  flashing is still one-way.** (And per project rules, stock images are never
  committed to this repo.)
