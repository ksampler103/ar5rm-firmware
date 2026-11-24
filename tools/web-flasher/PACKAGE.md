# M0 flash package — preparation record

Prepared 2026-11-24 on clt-build-01. **Nothing was flashed and no serial port
was opened toward any radio** — verification was purely offline (file parsing
and crypto round-trips). The `.BF` binary itself is deliberately **not
committed** to this repo; it exists at:

- Build box: `~/ar5rm-firmware/bf5rh-m0.BF`
- This VM: `~/workspace/ar5rm-firmware-repo/tools/web-flasher/bf5rh-m0.BF`
  (local, untracked — verify its hash against this file before any use)

## Inputs and outputs

| File | Size | SHA-256 |
|---|---|---|
| Input raw binary `bf5rh-fw-m0.bin` (M0 cargo build, bf-5rh `e52b236`) | 57,492 B | `cf3a906530865fa0e34796efa78205886b91c4ab9042d46d5f7ebd87ad06fc1d` |
| Output package `bf5rh-m0.BF` | 57,508 B (= 16-byte header + 57,492) | `b04356cbeebb91a1aa0b593e5e1414df1fc1a3cc6fae716b1f0c36875c90357e` |

SHA-256 prefix for the mod `flash_bf.py` gate argument: **`b04356cbeebb91a1`**.

## Wrap command

Built amoxu's tool from the cloned RE repo (`cc03870e0e29d9ded34a55d907d00167e8e0bb6f`)
and ran, on the build box:

```sh
cd ~/ar5rm-firmware/Baofeng-UV-5RM-5RH-RE/reverse_engineering/uv5rm-wrap-tool
make        # gcc -g -Wall -Wextra -o uv5rm-wrap-tool encrypt.c main.c
./uv5rm-wrap-tool -m wrap -i ~/ar5rm-firmware/bf5rh-fw-m0.bin \
                  -o ~/ar5rm-firmware/bf5rh-m0.BF -f
```

No `-d` data file → one-region package (`cntPart = 1`, `lenData = 0`), as
prescribed by the tool's README for a firmware-only wrap. Tool identity:
`uv5rm-wrap-tool` built from amoxu repo commit `cc03870` (sources
`main.c`/`encrypt.c`/`encrypt.h` in `reverse_engineering/uv5rm-wrap-tool/`).

## Verification performed (all offline)

1. **Header sanity** (per PROTOCOL.md §6.1): first 16 bytes =
   `01 0000e094 00000000 00000000000000` → `cntPart = 1`, `lenFw = 57,492`
   (0xE094, big-endian), `lenData = 0`, reserved all zero (wrap-tool style).
   File size 57,508 = 16 + 57,492 + 0. ✔
2. **bf-5rh `tools/flash.py` parser**: `_parse_bf()` imported and run on the
   file with no serial port involved (pyserial absent on the box; a stub
   module satisfied the import — `_parse_bf` is pure file parsing). Result:
   region 1 = 57,492 B, region 2 = 0 B; 57 packages at 1024 B; within the
   `REGION1_MAX_BYTES = 0xF000` limit. ✔ (flash.py has no dedicated
   dry-run/verify mode — its `main()` requires a port — so its parser was
   exercised directly instead.)
3. **amoxu unwrap round-trip**: `uv5rm-wrap-tool -m unwrap` on the produced
   `.BF` yields a file byte-identical to the input raw binary (`cmp` clean). ✔
4. **Independent crypto cross-check**: the mod's `wrap_bf.py` `crypt()`
   applied to the package's region-1 payload reproduces the original raw
   binary exactly; the decrypted vector table reads SP = `0x20004000`,
   reset = `0x080010D5` (in range, thumb bit set). ✔
5. **Mod flasher pre-flight asserts** (checked by hand against
   `flash_bf.py:31-34`): `cntPart == 1`, `lenData == 0`,
   file length == 16 + lenFw, `0 < lenFw ≤ 0xF000`, encrypted
   `region[3] == 0x20` and `region[7] == 0x08`. All pass. ✔

## Status

Package is ready to be *offered* to the web flasher / `flash_bf.py`. Using it
still requires Preston's separate explicit go for that specific flash, a
completed cable backup first, and a passing bootloader probe (see
`../../m0/BOARD-ID-CHECKLIST.md` §7).
