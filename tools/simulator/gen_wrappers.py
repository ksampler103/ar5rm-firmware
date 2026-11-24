#!/usr/bin/env python3
"""Generate host-build wrapper crates for bf-5rh overlay apps.

An overlay app is a `#![no_std] #![no_main]` binary crate whose only
entry point is `app_entry(api, ev)`. To run it on a PC we:

  1. copy the app's source tree into `gen/srcs/<app>/`,
  2. transform each segment's `src/main.rs` into an ordinary module:
       - drop the `#![no_std]` / `#![no_main]` inner attributes,
       - drop `#[no_mangle]` (the harness calls the fn by module path;
         keeping it would collide when several apps share a workspace),
       - remove the `#[panic_handler]` fn (the host has its own),
       - in a shared `common.rs`, rewrite `crate::MY_SEG` to
         `super::MY_SEG` (the segment root is a module here, not the
         crate root),
  3. emit `gen/<app>/wrap/` — a lib crate exposing each transformed
     segment main as `seg0`, `seg1`, ... via `#[path]`,
  4. emit `gen/<app>/` — a `sim-<app>` binary crate whose `main` hands
     the segment entries to `bf5rh_sim::main_for`.

Upstream app sources are copied, never modified in place. Set
BF5RH_TREE to the bf-5rh-firmware-open checkout (default
~/ar5rm-firmware/bf-5rh-firmware-open). Custom apps are read from this
repo's tools/web-flasher/custom-apps/.

Usage: python3 gen_wrappers.py [app ...]   (default: all known apps)
"""

import os
import re
import shutil
import struct
import sys
from pathlib import Path

SIM_DIR = Path(__file__).resolve().parent
GEN = SIM_DIR / "gen"
UPSTREAM = Path(
    os.environ.get(
        "BF5RH_TREE", Path.home() / "ar5rm-firmware" / "bf-5rh-firmware-open"
    )
)
CUSTOM = Path(
    os.environ.get(
        "BF5RH_CUSTOM_APPS", SIM_DIR.parent / "web-flasher" / "custom-apps"
    )
)

# name -> (source app dir, [segment crate subdirs], needs sgp4mini)
APPS = {
    "hello": (UPSTREAM / "apps/hello", ["."], False),
    "fm": (UPSTREAM / "apps/fm", ["."], False),
    "scanqt": (UPSTREAM / "apps/scanqt", ["."], False),
    "search": (UPSTREAM / "apps/search", ["."], False),
    "spectrum": (UPSTREAM / "apps/spectrum", ["."], False),
    "chanmgr": (UPSTREAM / "apps/chanmgr", ["."], False),
    "settings": (
        UPSTREAM / "apps/settings",
        ["menu", "radio", "system"],
        False,
    ),
    "satellite": (
        UPSTREAM / "apps/satellite",
        ["list", "predict", "track"],
        True,
    ),
    "actlog": (CUSTOM / "actlog", ["."], False),
    "rptbook": (CUSTOM / "rptbook", ["."], False),
    "foxhunt": (CUSTOM / "foxhunt", ["."], False),
    "wxpanel": (CUSTOM / "wxpanel", ["."], False),
    "timesync": (CUSTOM / "timesync", ["."], False),
}


def transform_main(text: str, where: str) -> str:
    # Drop inner attributes and #[no_mangle].
    out_lines = []
    for line in text.splitlines(keepends=True):
        stripped = line.strip()
        if stripped in ("#![no_std]", "#![no_main]", "#[no_mangle]"):
            continue
        # File modules become pub so the harness can reach items the app
        # exposes from a submodule (settings keeps app_entry in common.rs).
        m = re.match(r"^(\s*)mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;\s*$", line.rstrip("\n"))
        if m:
            line = f"{m.group(1)}pub mod {m.group(2)};\n"
        out_lines.append(line)
    text = "".join(out_lines)
    # Remove the #[panic_handler] item (brace-matched).
    m = re.search(r"#\[panic_handler\]", text)
    if m:
        brace = text.index("{", m.start())
        depth = 0
        i = brace
        while True:
            c = text[i]
            if c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
                if depth == 0:
                    break
            i += 1
        text = text[: m.start()] + text[i + 1 :]
    if "panic_handler" in text:
        print(f"[warn] {where}: a panic_handler mention survived transform")
    return text


def extract_features(cargo_toml: Path) -> str:
    """Return the [features] section of a manifest, verbatim (or '')."""
    if not cargo_toml.exists():
        return ""
    lines = cargo_toml.read_text().splitlines()
    out = []
    in_features = False
    for line in lines:
        if line.strip() == "[features]":
            in_features = True
            out.append(line)
            continue
        if in_features:
            if line.strip().startswith("["):
                break
            out.append(line)
    return "\n".join(out).strip() + "\n" if out else ""


def copy_tree(src: Path, dst: Path) -> None:
    if dst.exists():
        shutil.rmtree(dst)
    shutil.copytree(
        src,
        dst,
        ignore=shutil.ignore_patterns("target", "Cargo.lock"),
    )


def package_sizes(name: str) -> list[int] | None:
    """Per-segment (image_len + bss_len) from the built .app package.

    The harness enforces the firmware loader's size rules with these
    real numbers: oversized segments may not stay resident. Package
    header is 108 bytes: segment table at offset 28, 20 bytes per
    entry (offset, image_len, bss_len, entry_off, crc32).
    """
    candidates = [
        SIM_DIR.parent / "web-flasher" / "apps" / f"{name}.app",
        UPSTREAM / "apps" / f"{name}.app",
    ]
    for cand in candidates:
        if not cand.exists():
            continue
        data = cand.read_bytes()[:108]
        if len(data) < 108 or data[:4] != b"OVL1":
            continue
        count = data[8]
        sizes = []
        for i in range(count):
            image_len, bss_len = struct.unpack_from("<II", data, 28 + i * 20 + 4)
            sizes.append(image_len + bss_len)
        return sizes
    return None


def gen_app(name: str, app_dir: Path, segments: list[str], sgp4: bool) -> None:
    if not app_dir.is_dir():
        print(f"[skip] {name}: source dir {app_dir} not found")
        return
    srcs = SIM_DIR / "gensrcs" / name
    copy_tree(app_dir, srcs)
    # Transform every Rust file in the copy: segment mains lose their
    # crate-root attributes / #[no_mangle] / panic handler, and a shared
    # common.rs may carry a panic handler of its own (settings does).
    for rs in srcs.rglob("*.rs"):
        rs.write_text(transform_main(rs.read_text(), str(rs)))
    # Shared common.rs (multi-segment apps): crate:: -> super:: for MY_SEG.
    for common in srcs.rglob("common.rs"):
        t = common.read_text()
        if "crate::MY_SEG" in t:
            common.write_text(t.replace("crate::MY_SEG", "super::MY_SEG"))

    # One wrapper lib crate per segment: cargo features are per-crate,
    # and multi-segment apps (settings) use features to specialise the
    # shared common.rs per segment — the segment's own [features] table
    # is transplanted into its wrapper manifest verbatim.
    wrap_pkgs = []
    for i, seg in enumerate(segments):
        wrap = GEN / name / f"wrap{i}"
        (wrap / "src").mkdir(parents=True, exist_ok=True)
        deps = ['bf5rh-abi = { path = "../../../abi" }']
        if sgp4:
            deps.append('sgp4mini = { path = "../../sgp4mini" }')
        seg_toml_path = app_dir / ("" if seg == "." else seg) / "Cargo.toml"
        features = extract_features(seg_toml_path)
        manifest = (
            f'[package]\nname = "appwrap-{name}-seg{i}"\nversion = "0.0.0"'
            f'\nedition = "2021"\n\n'
        )
        if features:
            manifest += features + "\n"
        manifest += "[dependencies]\n" + "\n".join(deps) + "\n"
        (wrap / "Cargo.toml").write_text(manifest)
        rel = (
            f"gensrcs/{name}/src/main.rs"
            if seg == "."
            else f"gensrcs/{name}/{seg}/src/main.rs"
        )
        (wrap / "src" / "lib.rs").write_text(
            "// Generated by gen_wrappers.py — do not edit.\n"
            f'#[path = "../../../../{rel}"]\npub mod seg;\n'
        )
        wrap_pkgs.append(f"appwrap-{name}-seg{i}")

    bin_dir = GEN / name
    (bin_dir / "src").mkdir(parents=True, exist_ok=True)
    bin_deps = ['bf5rh-sim = { path = "../../harness" }']
    for i, pkg in enumerate(wrap_pkgs):
        crate = pkg.replace("-", "_")
        bin_deps.append(f'{crate} = {{ package = "{pkg}", path = "./wrap{i}" }}')
    (bin_dir / "Cargo.toml").write_text(
        f'[package]\nname = "sim-{name}"\nversion = "0.0.0"\nedition = "2021"\n\n'
        f"[dependencies]\n" + "\n".join(bin_deps) + "\n\n"
        f'[[bin]]\nname = "sim-{name}"\npath = "src/main.rs"\n'
    )
    # Most apps define app_entry in their segment main; settings keeps it
    # in the shared common.rs instead.
    entry_path = "common::app_entry" if name == "settings" else "app_entry"
    entries = ", ".join(
        f"appwrap_{name}_seg{i}::seg::{entry_path}" for i in range(len(segments))
    )
    sizes = package_sizes(name)
    if sizes is None:
        print(f"[warn] {name}: no built .app package found — loader size rules not enforced")
        sizes = []
    sizes_lit = ", ".join(str(s) for s in sizes)
    (bin_dir / "src" / "main.rs").write_text(
        f"// Generated by gen_wrappers.py — do not edit.\n"
        f'fn main() {{\n    bf5rh_sim::main_for("{name}", &[{entries}], &[{sizes_lit}]);\n}}\n'
    )
    print(f"[ok] {name}: {len(segments)} segment(s) wrapped")


def gen_sgp4mini() -> None:
    src = UPSTREAM / "sgp4mini"
    dst = GEN / "sgp4mini"
    if not src.is_dir():
        print(f"[skip] sgp4mini: {src} not found")
        return
    copy_tree(src, dst)
    # Packaging tweaks for the copy only: drop its standalone [workspace]
    # (it becomes a member of the sim workspace via the gen/* glob) and
    # its dev-dependencies (only used by its own tests; avoids an extra
    # crates.io fetch).
    toml = dst / "Cargo.toml"
    text = toml.read_text()
    text = text.replace("[workspace]\n", "")
    text = re.sub(r"\[dev-dependencies\][^\[]*", "", text)
    toml.write_text(text)
    print("[ok] sgp4mini copied")


def main() -> int:
    names = sys.argv[1:] or list(APPS)
    GEN.mkdir(exist_ok=True)
    need_sgp4 = False
    for name in names:
        if name not in APPS:
            print(f"[err] unknown app {name!r}; known: {', '.join(APPS)}")
            return 1
        app_dir, segments, sgp4 = APPS[name]
        need_sgp4 = need_sgp4 or sgp4
        gen_app(name, app_dir, segments, sgp4)
    if need_sgp4:
        gen_sgp4mini()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
