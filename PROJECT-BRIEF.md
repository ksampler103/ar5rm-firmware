# AR-5RM Custom Firmware Project — Brief

Started 2026-11-24 (spun off from Preston's radio chat). Owner: Preston Hunter.
Muse writes/builds on the build container; Preston does all hardware, flashing, and testing.

## Goal
Port the open-source **bf-5rh** firmware to Preston's AR-5RM units — specifically his
dev unit's **5RH10W-V05** board — as the first step toward a full-featured, RadioFoundry-style open
firmware for the radio. The radio is **analog FM only** (no vocoder hardware; DMR/NXDN/P25
are impossible and out of scope). Preston knows this; it is a project for its own sake.

## Hard rules
- **Nothing gets flashed to a radio without Preston's separate explicit go for that flash.**
- Unit 1 (BK) stays stock and unflashed — it is the reference radio. (Case opened once by accident, Nov 24, 2026 — see teardown note.)
- Unit 2 (NRF) is the designated dev/sacrificial unit.
- Respect upstream licenses; check bf-5rh and mod licenses before reusing code.
- Never commit stock firmware images or decrypted factory material to a public repo.

## Preston's units (both firmware v0.14)
| Unit | Hold-8 screen | Role |
|---|---|---|
| 1 "BK" | `5rhbk ver v0.14 bf` | His user (the Nov 15 NXDN ear-test radio). Stock and unflashed — the reference radio. Case opened once by accident (Nov 24, 2026; the NRF was the intended teardown target): dropped, keypad flex latch broken, flex shimmed with paper+tape, working. |
| 2 "NRF" | `5RHNRF / Ver v0.14 / bf` | Spare. Designated dev unit. Opened Nov 24, 2026: board is **5RH10W-V05** (silkscreen date 23.10.12) — a different board code from the BK's 5RB10W-V02 and further from amoxu's documented V01. **RF transceiver is an FD6818** (the DM-32UV's chip — documented part, open driver code in the RadioFoundry tree); LCD flex T-2287-A / S-6883-A (14 contacts) shares its 6883 code with the BK's module — panel possibly common to both boards. |

## Hardware (teardown 2026-11-24 of the BK unit — see `teardown/TEARDOWN-2026-11-24.md` + photos; the NRF dev unit's board is 5RH10W-V05 — its silicon is inventoried at the end of this section)
- PCB **5RB10W-V02** (purple; shield-can lands, no cans fitted) — same family as the
  RE-documented 5RB10W **V01**, one revision newer.
- MCU: LQFP-48, marking unreadable; expected customized Artery **AT32F421C8T7**
  (silkscreen SYN2A-000; Cortex-M4 120 MHz, 64K flash / 16K RAM, no Artery ISP bootloader).
  SWD IDCODE read will confirm.
- Flash map: 4 KB bootloader at base, **60 KB application at 0x08001000**. Bootloader takes
  **encrypted** update packages over the K-connector serial port; Flash Access Protection
  on by default. SOIC-8 beside the MCU presumed XMC **25QH16** (16 Mbit SPI flash).
- PA module: QFN house-marked **HTL7G06S01P / ETWT00D1 / 2414-JY** (no public part
  match). Originally read as the RF transceiver; the NRF board's layout (two identical
  HTL7G06S011P, one per band, in the PA section) identifies these as PA modules. The BK
  board's actual transceiver was never read (chips unreadable that night) and is unknown
  — irrelevant, as the unit stays stock.
- LCD: module **GD018QQ198S-FPC-A0** (code 6883), **14-contact** flex — serial (SPI-class)
  interface. Controller is chip-on-glass, unidentified (also blank in amoxu's table).
  Display bring-up is the expected fiddliest phase.

**NRF dev board (5RH10W-V05) silicon — photo-verified 2026-11-24** (teardown note +
`nrf-board-bottom.jpg`, `nrf-board-top.jpg`, `nrf-pa-section.jpg`):
- **RF transceiver: FD6818** (`FD6818 / CS3382XA`) with a 26.0 MHz reference crystal —
  the same transceiver as the DM-32UV: a documented part with open driver code in the
  RadioFoundry tree (incl. setup-B register tables). **This board is not a BK4819
  design** — bf-5rh's RF driver does not apply; the port is bf-5rh's application layer
  over a new FD6818 driver. This also plausibly explains the mod's don't-flash list for
  `...NRF` units.
- PA: 2x **HTL7G06S011P** modules (one per band). SPI flash: **XMC 25QH16 confirmed**.
  Broadcast FM tuner: 5807M (RDA5807M family). MCU: LQFP-48 under a red factory mark,
  top text illegible — SWD IDCODE will confirm AT32F421.

## Existing work to build on
- **amoxu/Baofeng-UV-5RM-5RH-RE** — the platform RE: encryption defeated (first 2 KB of a
  package is unencrypted; a sub-2 KB dumper firmware reads the bootloader out over serial),
  decryption tools, V01 teardown photos, factory firmware images for *some* variants.
- **bf-5rh** — open-source Rust firmware for this platform family.
  **jakegarrett13-afk/UV-5RM-Radio-Mod** is a shipped build on it (radio-to-radio texting
  with delivery confirmation, spectrum waterfall, band sweep, repeater finder, fox hunt,
  scan banks) — but its compatibility gate is hold-8 string `5RHNRFB-YP07` only; the author
  warns `...NRF` and `...BK` units "may boot but not transmit or receive." **Neither of
  Preston's units qualifies** — hence this port.
- **FlintWave KDH flasher** catalog confirms the UV-5RM's KDH bootloader (BK4819 RF,
  5RB10W hardware, firmware v0.09+) — that entry describes the BK4819 variant; Preston's
  dev unit is the FD6818 variant (same bootloader family expected; verify at M0).
- **Recovery:** full SWD wipe-and-reflash demonstrated on this family (porkfreezer's
  GM-5RH writeup; Artery OpenOCD fork; SWD pads on the PCB share pins with the RX/TX LEDs;
  reset pin must be held). A bricked dev unit is recoverable for *our* firmware.
- **The closed door:** returning to *stock*. No v0.14 factory image is known for either of
  Preston's variants; flashing is one-way off stock. Config backup over the cable before
  any flash (flash mode cannot read the firmware).

## Route
- **Route A (chosen): port bf-5rh.** The port is a board-definition job: display
  controller/pin mapping first, then keypad and RF-switch control. First experiment =
  flash the least-modified build to the dev unit and catalog what works — that partitions
  shared platform from port list. On the FD6818 dev board, "port" means bf-5rh's
  application/UI layer over a new FD6818 driver (register reference: RadioFoundry's DM32
  FD6818 code + the FD6818 datasheet), not a straight bf-5rh build.
- **Route B (fallback): write our own** at UV-K5-firmware scale (the FD6818 is
  documented, with RadioFoundry driver code as the reference). Months of evenings; only
  if Route A fails.
- A literal RadioFoundry port is impossible on flash math (~580 KB images vs 60 KB
  application space); "something *like* RadioFoundry" — open, full-featured, real
  UI/menus/settings, host tools — is the target, at UV-K5 scale.

## Plan (M0 first — mirrors the DM32 prep pattern)
1. Clone amoxu's RE repo + bf-5rh source on clt-build-01 (LXC 114, 10.0.10.30; SSH always
   `ssh -F /home/hatch/.ssh/config` via jump-phx). Set up the Rust toolchain; get bf-5rh
   compiling clean. Write the board-identification checklist.
2. Draft the 5RH10W-V05 board definition from the teardown note: known vs assumed vs
   must-test (FD6818 variant/setup tables, display init, keypad map, RF switch control).
   Pull the FD6818 documentation from the RadioFoundry tree already on the build box
   (DM32 project) plus the datasheet.
3. **First-flash experiment — only on Preston's explicit order.** Least-modified build to
   the NRF unit; catalog what works.
4. Display bring-up, then features in slices.

## Feasibility (assessed 2026-11-24)
Feasible via Route A: RE done upstream, working firmware for the family, two units
(dev + reference), SWD recovery demonstrated, and the Muse-builds / Preston-flashes
workflow already proven on the DM32 project. The dev board (5RH10W-V05) is a different
board code from the documented 5RB10W-V01, but its transceiver turned out to be the
documented FD6818 rather than an unknown part — the RF risk inverted into an asset.
Remaining risk #1 is the display controller identity, resolved at display bring-up.
