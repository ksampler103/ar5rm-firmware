# M0 build log — 2026-11-24

Environment: build container **clt-build-01** (LXC 114, 10.0.10.30), work dir
`~/ar5rm-firmware/` on the box. Preston approved processing/building the upstream
repos in chat on 2026-11-24. No radio contact, no flashing, no SWD in M0.

## Repositories cloned (on clt-build-01, `~/ar5rm-firmware/`)

| Repo | URL | Commit |
|---|---|---|
| amoxu RE | https://github.com/amoxu/Baofeng-UV-5RM-5RH-RE | `cc03870e0e29d9ded34a55d907d00167e8e0bb6f` |
| bf-5rh (standalone upstream) | https://github.com/sophiel-meow/bf-5rh-firmware-open | `e52b2360af7b3152df72f1255682503a0555bb98` ("spectrum app") |
| jakegarrett mod (reference) | https://github.com/jakegarrett13-afk/UV-5RM-Radio-Mod | `d605767763edda6b9bb094cfd25b885dddf8a892` |

The standalone bf-5rh repo identity was verified via the GitHub API
(`sophiel-meow/bf-5rh-firmware-open`, "Open re-implementation of the Baofeng
UV-5RH firmware", GPL-3.0, default branch `master`). The mod repo bundles a copy
of the same source under `UV-5RM Custom Firmware/firmware-source/`; the build
below used the **standalone upstream**, not the mod's bundle.

## Toolchain

- Installed via rustup into `/root/.cargo` on the box (log:
  `~/ar5rm-firmware/rustup-install.log`).
- `rustc 1.100.0 (0ef1bc386 2026-11-09)`, `cargo 1.100.0`.
- bf-5rh's `rust-toolchain.toml` pins `channel = "stable"` with target
  `thumbv7em-none-eabi` — installed and used as pinned.
- `llvm-tools` component added afterwards only to get `llvm-objcopy` for the
  raw-binary size measurement (upstream's documented `cargo objcopy` needs the
  `cargo-binutils` crate, which was not installed; the README's documented
  environment is `nix develop`, and Nix is not on this box — plain cargo with
  the pinned toolchain worked without it).

## Build

Command (in `~/ar5rm-firmware/bf-5rh-firmware-open`):

```sh
cargo build --release
```

Result: **success, exit 0** (log: `~/ar5rm-firmware/bf5rh-build.log` on the box).
11 warnings, all benign for M0 purposes: dead-code/unused items, one deprecated
`cortex_m_rt::pre_init` macro, and one linker note (`.text` address alignment).
No upstream code was patched.

### Size vs the 60 KB application limit

| Artifact | Size |
|---|---|
| ELF `target/thumbv7em-none-eabi/release/bf5rh-fw` (with debug info) | 1,177,840 B |
| Raw binary (llvm-objcopy `-O binary`), copy kept on box as `~/ar5rm-firmware/bf5rh-fw-m0.bin` | **57,492 B** |
| Application region limit (`memory.x`: FLASH @ 0x08001000, LENGTH 60K) | 61,440 B |
| **Spare** | **3,948 B** |

The stock upstream build fits the 60 KB region, with ~3.9 KB headroom. That is
tight: feature growth on this port will be a budgeting exercise from day one.

## Notable source findings (drive the other M0 docs)

- Upstream bf-5rh is **not** a BK4819 firmware: `src/drivers/` contains
  `fd6818b.rs` (1,705 lines), `st7735.rs` (LCD, 160×128), `rda5807.rs` (FM
  tuner), `norflash.rs`. Its FD6818 driver comments cite the **5RH NRF v0.14
  stock firmware disassembly** (`nrf014.dis`) as a source — the upstream author
  reverse-engineered the same variant family as Preston's dev unit.
- The board/pin layer is `src/board/*.rs` (rf, lcd, keypad, power, storage,
  fm, leds, flashlight, serial) over `src/hal/*.rs`, with the AT32F421 PAC in
  `at32f421-pac/`. See `BOARD-5RH10W-V05.md` for the mapping.
- The amoxu RE repo's `factory_firmware/` contains an **NRF FD6818 folder with
  v0.14**: `NRF_FD6818/5RH_NRF_Fangti_V0.14_231124.BF` (60,704 B) and v0.13.
  This contradicts the brief's "no v0.14 factory image known" — see the
  checklist doc for the caveats before treating it as a restore path.

## Blockers / open items

- None for the M0 build itself.
- FD6818 datasheet PDF: **not present** in the RadioFoundry tree on the box and
  not downloaded in M0 (marked unavailable; the register knowledge in both
  codebases comes from stock-firmware RE and the RadioFoundry spec docs).
- Whether upstream bf-5rh's board definition matches the 5RH10W-V05 board
  pin-for-pin is unproven — that is the first-flash experiment's job (separate
  explicit go required).
