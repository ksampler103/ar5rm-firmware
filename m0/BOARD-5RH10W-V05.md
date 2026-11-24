# Board definition draft — 5RH10W-V05 (AR-5RM "NRF" dev board)

Status: **draft, M0 (2026-11-24)**. No code has run on this board. Everything
below is either photo-verified, sourced from upstream code/docs, or explicitly
marked must-test.

The surprise of M0: upstream bf-5rh (`sophiel-meow/bf-5rh-firmware-open`,
`e52b236`) is already an FD6818 firmware — `src/drivers/fd6818b.rs` (1,705
lines) cites the **NRF v0.14 stock firmware disassembly** (`nrf014.dis`) as a
primary source. So the "board definition" largely already exists upstream in
`src/board/*.rs`; the port question is whether it matches *this* board
(5RH10W-V05) pin-for-pin, not whether it must be written from scratch.

Where a board definition lives in bf-5rh (real paths, cloned source):

- `src/board/` — pin-level board layer: `rf.rs` (FD6818 3-wire pins + speaker
  amp), `lcd.rs` (LCD control + backlight pins), `keypad.rs` (matrix + side
  key), `power.rs` (power latch/key), `storage.rs` (NOR flash SPI pins),
  `fm.rs` (RDA5807 SCL), `leds.rs` (TX/RX LEDs), `flashlight.rs`, `serial.rs`.
- `src/drivers/` — chip drivers: `fd6818b.rs`, `st7735.rs`, `rda5807.rs`,
  `norflash.rs`.
- `src/hal/` — MCU glue: `clock.rs`, `adc.rs`, `uart.rs`, `delay.rs`,
  `uptime.rs`, `debounce.rs`, `wear_leveled.rs`, `stackprobe.rs`.
- `src/device/` — device abstractions over board+drivers (`radio.rs`,
  `display.rs`, `keypad.rs`, `storage.rs`, `fm_radio.rs`, `flashlight.rs`).
- `memory.x` — flash/RAM map; `at32f421-pac/` — the MCU PAC crate;
  `src/flash_map.rs` — external-flash layout.

## KNOWN (photo-verified on Preston's dev unit, teardown 2026-11-24)

- PCB **5RH10W-V05**, silkscreen also `2350.` and date `23.10.12`; purple,
  shield-can lands, no cans fitted.
- RF transceiver **FD6818** (`FD6818 / CS3382XA`) with **26.0 MHz** reference
  crystal (`YL260 / AK1D`).
- PA: **2× HTL7G06S011P** house-marked modules, one per band, in the
  antenna-end RF section with air-core coils and trimmer caps.
- External SPI flash: **XMC 25QH16** (`25QH16CJIG`), 16 Mbit, SOIC-8 by the MCU.
- Broadcast FM tuner: **5807M** (RDA5807M family), `TM786`.
- MCU: **LQFP-48**, top text illegible under a red factory mark.
- LCD flex: **T-2287-A / S-6883-A / 2211 HT**, **14 contacts** (pin 1/14
  markers). Shares code **6883** with the BK unit's module
  (`GD018QQ198S-FPC-A0`) — same panel family suspected, unproven.
- Power switching: 3415J MOSFETs; assorted R25/A1-marked RF transistors in the
  front end.

## ASSUMED (from upstream code / platform docs — source tagged per item)

- **MCU is AT32F421C8T7** (Cortex-M4 120 MHz, 64K flash / 16K RAM, customized,
  no Artery ISP bootloader). Sources: amoxu hardware table (SYN2A-000 =
  customized AT32F421C8T7); bf-5rh builds entirely on its `at32f421-pac`.
  Confirmation = SWD IDCODE (see checklist §4; value to-confirm).
- **Flash map:** 4 KB KDH bootloader at 0x08000000, **60 KB app at
  0x08001000**, config in the system-bootloader region at 0x1FFFE400. Sources:
  amoxu README; bf-5rh `memory.x` (FLASH ORIGIN 0x08001000 LENGTH 60K, RAM
  16K); bf-5rh `tools/flash.py` enforces region-1 max 0xF000.
- **Bootloader accepts encrypted `.BF` packages over the K-connector serial
  port; flash access protection on by default.** Source: amoxu README.
- **FD6818 bus (bf-5rh `board/rf.rs`):** bit-banged 3-wire — LE **PC13**,
  SCK **PA2**, SDIO **PA3**; PA3 is shared with the RDA5807's bit-banged I2C
  SDA. Speaker amp enable **PB7** active high (same file).
- **Display (bf-5rh `board/lcd.rs` + `drivers/st7735.rs`):** ST7735-class
  controller, **160×128**, SPI2 (PB13 SCK / PB15 MOSI, AF0), CS **PB2**,
  RST **PB12**, DC **PB14**, backlight **PA4**. MADCTL 0x60 in the driver init.
- **Keypad (bf-5rh `board/keypad.rs`):** matrix columns driven on **PB8–PB11**
  (idle high, one pulled low per phase), rows read on **PB3–PB6** (inputs,
  pull-up); side key 2 on **PA15** (active low). Power key **PB0**, power
  latch **PF6** (`board/power.rs`).
- **External flash bus (bf-5rh `board/storage.rs`):** SPI1 — SCK **PA5**,
  MISO **PA6**, MOSI **PA7** (AF0), CS **PA8**.
- **FM tuner bus (bf-5rh `board/fm.rs`):** RDA5807 SCL **PC15**, SDA shared on
  PA3 (see FD6818 bus above).
- **LEDs (bf-5rh `board/leds.rs`):** TX **PA13**, RX **PA14** — the same pins
  as SWD (SWDIO/SWCLK), consistent with the brief's "SWD pads share pins with
  the RX/TX LEDs".
- **Battery ADC (bf-5rh `device/radio.rs`):** battery voltage on **PA1 =
  ADC_IN1**; a VOX ADC channel also exists in that module.
- **FD6818 register model:** bf-5rh's driver implements the full analog-FM
  feature set against the FD6818 register map (state word REG 0x30 with
  RX 0xBFF1 / TX 0xC1FE, freq REG 0x38/0x39, squelch REG 0x78, RSSI REG 0x67,
  CTCSS/DCS incl. Golay(23,12) encoding and off-air tone decode, DTMF, FSK,
  MDC1200, scramble, hardware frequency-scan). Its constants were cross-checked
  by its author against the NRF v0.14 stock disassembly — i.e. this register
  layer is *for this variant family*, not a guess.

## MUST-TEST / UNKNOWN (each with a proposed test)

1. **Pin-map match, whole board.** Upstream's `src/board/*.rs` pins (above)
   come from the upstream author's unit(s), board revision unknown to us.
   *Test:* the first-flash experiment itself, staged — boot + backlight +
   display first, then keypad scan, then RF — cataloguing each function
   separately (Preston's explicit go required; nothing in M0 authorizes it).
2. **FD6818 setup-table variant (default vs "setup B").** RadioFoundry found
   two FD6818 populations: one locks on the default init tables, the other
   needs setup B (different init/FM/DMR tables; 0x37 bit 15 set; extra 0x30
   step). bf-5rh has a single baked-in init sequence in `Fd6818::init`.
   *Test:* at first RF bring-up, check PLL lock (REG 0x0C bit 9) and RX on a
   known signal after bf-5rh init. No lock/dead RX with correct pins → port
   RadioFoundry's `fd6818_detect` try-each-variant pattern and the setup-B
   tables (`drivers/rf/fd6818/fd6818_setup_b.c`).
3. **Display controller identity.** COG, unnamed on both Preston units and in
   amoxu's table ("unknown yet"). bf-5rh assumes ST7735 160×128.
   *Test:* first-flash display phase. Legible UI = ST7735-compatible, done.
   Garbage/shifted colors = try MADCTL/orientation and init-sequence variants
   from the ST7735/ST7789 family before concluding it's a different COG.
4. **Keypad map.** Matrix wiring assumed as in `board/keypad.rs`; the
   logical key layout (which row/col = which key) lives in `src/app/keys.rs`
   + `src/device/keypad.rs`.
   *Test:* keypad phase of first flash — walk every key against the map,
   record swaps. (Note: the *BK* unit's shimmed flex is a red herring here;
   the dev unit's flex is intact.)
5. **RF band-switch / PA control.** The dev board has two discrete PA modules;
   bf-5rh switches bands via FD6818 GPIO bits driven from the driver
   (`set_gpio_bit`, `select_band`, `pa_enable` in `fd6818b.rs`) — i.e. band
   and PA selects are FD6818 GPIO outputs, not MCU GPIOs, in this design.
   *Test:* TX into a dummy load on both bands at low power with a power
   meter, only after RX is proven; confirm the correct PA is enabled per band
   and neither PA is on during RX.
6. **Audio path.** FD6818 AFOUT → speaker amp (PB7 enable assumed). Mic path
   via FD6818 MICIN ADC (REG 0x30 bit 2) with deviation/mic-gain registers in
   the driver.
   *Test:* RX audio on a known signal, then TX deviation check on a service
   monitor or the reference radio; watch for the RadioFoundry-noted trap that
   a wrong REG 0x47 AF word leaves the speaker silent on a strong signal.
7. **Battery ADC scaling.** Channel assumed PA1/ADC_IN1; the divider ratio
   and calibration live in `device/radio.rs` / storage-held calibration
   (`read_battery_calibration`).
   *Test:* compare displayed voltage against a bench meter at two pack
   voltages; adjust scale factor only.
8. **SPI flash identity + layout.** Part confirmed by marking (25QH16), but
   bf-5rh's `flash_map.rs` layout (channels/settings/apps regions) is its own
   format — stock config is *not* preserved by adopting it, hence the
   mandatory pre-flash cable backup (checklist §7). JEDEC ID readback at
   first run closes the identity question.
9. **Flash-mode entry on this variant.** Documented upstream as
   hold-both-side-buttons + power-on (mod) / hold side key (bf-5rh README).
   Untested on the FD6818 variant.
   *Test:* handshake probe only (non-destructive) before any flash session.
10. **26 MHz reference trim.** FD6818 ref is 26.0 MHz on this board;
    bf-5rh has `set_xtal_adjust` + an adjust table.
    *Test:* frequency-error measurement on a known carrier vs the stock
      reference unit; set trim from measurement, not default.

## FD6818 driver scoping — RadioFoundry tree (for the port's reference shelf)

Tree: `~/dm32-firmware/RadioFoundry` on clt-build-01 (DM32 project; untouched).

| File | Lines | Role |
|---|---|---|
| `drivers/rf/fd6818/fd6818.c` / `.h` | 513 / 264 | Core driver over an abstract `rfbus_t` (register write/read + delay): attach/variant select, `fd6818_detect` (try variants, PLL-lock check), mode set (FM/DMR), state set (idle/RX/TX words 0x0000/0xBFF1/0xC1FE), band words, RSSI raw→dBm (`(raw>>1) − 160` class formula), tone (CTCSS/DCS) helpers |
| `drivers/rf/fd6818/fd6818_setup_b.c` | 80 | Setup-B variant tables (init / FM / DMR / DMR-alt) — the "second population" fix |
| `drivers/rf/fd6818/fd6818sh.c` / `.h` | 628 / 127 | Superhet-board variant layer (DM-1801 Pro / DB25-D class boards) |
| `drivers/rf/fd6818/fd6818sh_superhet.c` / `.h` | 75 / 13 | Superhet front-end glue |
| `tests/host/fd6818Test.c`, `fd6818shTest.c` | 745 / 358 | Host-side unit tests for the register logic |
| `boards/dm32/radio_dm32.c` | — | DM32 board glue: bit-banged FD6818 bus, variant detection at probe freq, LNA/PA selects, DMR burst TX/RX switching |

Bus on DM32: **bit-banged 3-wire** (same scheme bf-5rh uses), abstracted as
`rfbus_t` so the register logic is MCU-agnostic. Register addresses/values
agree with bf-5rh's map (0x30 state, 0x38/0x39 freq, 0x67 RSSI, 0x0C status
with lock bit 9) — two independent codebases converging on one register model.

Datasheet: **no FD6818 datasheet PDF in the RadioFoundry tree**, and none was
downloaded in M0. Knowledge in both trees derives from stock-firmware RE
(RadioFoundry's `docs/spec/hardware-dm32.md` §8.10, `hardware-dm1801pro.md`
§5; bf-5rh's `nrf014.dis` work). Marked **unavailable** for now.

### Port estimate (honest)

- **RF register logic: ~zero new work to start.** bf-5rh already contains a
  complete FD6818 analog-FM driver (1,705 lines) RE'd from this variant's own
  stock firmware. RadioFoundry's C driver (~1,300 lines + tests) is a
  cross-reference and the source of the setup-B fallback, not a porting base.
- **The real port risk is board glue, not RF:** if the 5RH10W-V05 pin map
  differs from upstream's board files, the fix is in `src/board/*.rs`
  (~370 lines total) — hours-to-days of staged bench work, dominated by the
  display question, not code volume.
- **Setup-B contingency:** if this board's FD6818 is a setup-B part, porting
  RadioFoundry's detect + tables into the Rust driver is a small, bounded job
  (tables are ~60 register pairs; detect is one function) — days, not weeks.
- **Flash budget is the standing constraint:** stock build already uses
  57,492 of 61,440 B. Any added table/feature must be bought with removals.
