# Overlay apps built for our bf-5rh firmware

Built 2026-11-24 on clt-build-01 from the **same upstream source tree as the
flashed firmware** (`bf-5rh-firmware-open` @ `e52b236`), using the repo's own
flow: `tools/mkapp.py` to pack, `tools/check_app.py` to validate. Because the
apps and the firmware share one `abi/src/lib.rs`, their `BUILD_HASH` matches
the firmware's by construction.

- Firmware ABI `BUILD_HASH`: **`0xDEC616E9`** (CRC-32 of `abi/src/lib.rs`)
- Package magic: `OVL1`; header 108 bytes; slot size 32,768 bytes (all fit)
- Suggested slots follow upstream `tools/install_all.py`'s default layout.

| File | App name | Segments | Size | SHA-256 | check_app verdict | Suggested slot |
|---|---|---|---|---|---|---|
| `apps/fm.app` | FM | 1 (image 3,692 B, bss 4 B) | 3,800 B | `54913aec9fdd575d5bd88faeb5b24581bb57cf02054f1b3d3955afd4d622c2e3` | PASS — build_hash, size, stack budget, crc all OK | 1 |
| `apps/settings.app` | SETTINGS | 3 — menu (6,392 B), radio (6,752 B), system (6,812 B); segment order menu=0, radio=1, system=2 per `apps/settings/common.rs` | 20,064 B | `56241c25308bfa09abd49fd28b706551cf8fb85d2510324e5bd560d7d3a469a7` | PASS — all checks OK | 6 |
| `apps/spectrum.app` | SPECTRUM | 1 (image 6,092 B, bss 1,348 B) | 6,200 B | `ff8d85aa9f9fb36512925f59ba4a62e265723928f1a6e1031c66c9d0ea0d9063` | PASS — all checks OK | 7 |
| `apps/chanmgr.app` | CHANMGR | 1 (image 7,700 B, bss 136 B) | 7,808 B | `2460505575c2bf60c9e7543bd06f8238f1582dc32f00b919ebeee46dd2382f07` | PASS — all checks OK | 2 |

Build notes:

- `mkapp.py` needs `cargo` plus the llvm-tools `rust-objcopy` / `rust-nm` /
  `rust-size` / `rust-objdump` on PATH. On clt-build-01 these live in the
  rustup sysroot's `lib/rustlib/x86_64-unknown-linux-gnu/bin/` (with
  `llvm-nm`/`llvm-objdump`/`llvm-size` symlinked as `rust-*` in
  `~/ar5rm-firmware/toolbin/`). First failure mode if missing: bare
  `FileNotFoundError: 'cargo'` / `'rust-objcopy'` from `mkapp.py`.
- The `settings` app is a 3-segment package; its crates must be passed to
  `mkapp.py` in the order `menu radio system` (segment indices are
  compile-time constants in `apps/settings/common.rs`).
- Validation used `check_app.py <file>.app` (header checks: build_hash,
  size, stack budget, per-segment CRC). The origin/relocation checks only
  run when packing from crate dirs and were not repeated — the packages
  came straight out of `mkapp.py` in the same tree.

## Commit decision

The four `.app` files **are committed** in this directory (`apps/`). They
are our own build outputs from GPL-3.0 upstream source, each is only a few
KB, and committing them means GitHub Pages (or any static host) can serve
them alongside the flasher page later — Preston's stated direction for this
tooling. They are **not** stock firmware and contain no radio data, so the
project rule against committing stock images does not apply. Rebuild them
from source rather than trusting the committed copies if the firmware's
`abi/src/lib.rs` ever changes — the web flasher rejects any package whose
build hash doesn't match the firmware it was built for.
