# ar5rm-firmware

Custom firmware for the **Baofeng AR-5RM** — porting the open-source **bf-5rh** firmware
to the **5RB10W-V02** board, as a first step toward a full-featured, RadioFoundry-style
open firmware for the radio.

The AR-5RM is analog FM only (no vocoder hardware). This is a project for its own sake.

## Status (2026-11-24)
- Platform reverse engineering: done upstream (amoxu/Baofeng-UV-5RM-5RH-RE).
- One unit torn down and documented — see `teardown/` (the BK unit, opened by accident; the NRF dev unit is still sealed and its board unverified).
- Route chosen: **port bf-5rh** (board-definition job), from-scratch firmware as fallback.
- Next: M0 prep — clone the RE repo + bf-5rh on the build box, get bf-5rh compiling
  clean, draft the 5RB10W-V02 board definition. **No radio has been flashed.**

## Layout
- `PROJECT-BRIEF.md` — the full project dossier: goal, units, hardware, route, plan, risks.
- `teardown/` — teardown notes and findings for the dev unit (board 5RB10W-V02,
  RF chip house-marking, LCD module + flex pinout).

## Rules
- Nothing is flashed to a radio without the owner's separate explicit go, per flash.
- The "BK" unit stays stock and unflashed as the reference radio (case opened once by accident — see the teardown note).
- No stock firmware images or decrypted factory material in this repo, ever.
