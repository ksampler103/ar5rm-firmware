# AR-5RM Custom Firmware Project — Brief

Started 2026-11-24 (spun off from Preston's radio chat). Owner: Preston Hunter.
Muse writes/builds on the build container; Preston does all hardware, flashing, and testing.

## Goal
Port the open-source **bf-5rh** firmware to Preston's AR-5RM units — specifically his
**5RB10W-V02** board — as the first step toward a full-featured, RadioFoundry-style open
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
| 2 "NRF" | `5RHNRF / Ver v0.14 / bf` | Spare. Designated dev unit. Still sealed, never opened — its board revision is unknown (the teardown data in this brief is from the BK unit's board). |

## Hardware (teardown 2026-11-24 of the BK unit — see `teardown/TEARDOWN-2026-11-24.md` + photos; the NRF unit's board is unverified)
- PCB **5RB10W-V02** (purple; shield-can lands, no cans fitted) — same family as the
  RE-documented 5RB10W **V01**, one revision newer.
- MCU: LQFP-48, marking unreadable; expected customized Artery **AT32F421C8T7**
  (silkscreen SYN2A-000; Cortex-M4 120 MHz, 64K flash / 16K RAM, no Artery ISP bootloader).
  SWD IDCODE read will confirm.
- Flash map: 4 KB bootloader at base, **60 KB application at 0x08001000**. Bootloader takes
  **encrypted** update packages over the K-connector serial port; Flash Access Protection
  on by default. SOIC-8 beside the MCU presumed XMC **25QH16** (16 Mbit SPI flash).
- RF transceiver: QFN in the shielded RF area, house-marked
  **HTL7G06S01P / ETWT00D1 / 2414-JY** (no public part match). **BK4819** identity is
  documented for this board position (amoxu, V01) but **unproven on this board** — risk #1,
  resolved at first code run via BK4819 register behavior.
- LCD: module **GD018QQ198S-FPC-A0** (code 6883), **14-contact** flex — serial (SPI-class)
  interface. Controller is chip-on-glass, unidentified (also blank in amoxu's table).
  Display bring-up is the expected fiddliest phase.

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
  5RB10W hardware, firmware v0.09+).
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
  shared platform from port list.
- **Route B (fallback): write our own** at UV-K5-firmware scale (BK4819 is the
  best-documented cheap RF chip in the hobby). Months of evenings; only if Route A fails.
- A literal RadioFoundry port is impossible on flash math (~580 KB images vs 60 KB
  application space); "something *like* RadioFoundry" — open, full-featured, real
  UI/menus/settings, host tools — is the target, at UV-K5 scale.

## Plan (M0 first — mirrors the DM32 prep pattern)
1. Clone amoxu's RE repo + bf-5rh source on clt-build-01 (LXC 114, 10.0.10.30; SSH always
   `ssh -F /home/hatch/.ssh/config` via jump-phx). Set up the Rust toolchain; get bf-5rh
   compiling clean. Write the board-identification checklist.
2. Draft the 5RB10W-V02 board definition from the teardown note: known vs assumed-from-V01
   vs must-test (RF register fingerprint, display init, keypad map, RF switch control).
3. **First-flash experiment — only on Preston's explicit order.** Least-modified build to
   the NRF unit; catalog what works.
4. Display bring-up, then features in slices.

## Feasibility (assessed 2026-11-24)
Feasible via Route A: RE done upstream, working firmware for the family, board one
revision off the documented one, two units (dev + reference), SWD recovery demonstrated,
and the Muse-builds / Preston-flashes workflow already proven on the DM32 project.
Risk #1 is the RF silicon identity (above) — and it resolves in week one, not month three.
