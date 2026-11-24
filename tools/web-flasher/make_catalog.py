#!/usr/bin/env python3
"""Regenerate docs/catalog.json for the AR-5RM web flasher's built-in catalog.

Scans docs/firmware/*.BF and docs/apps/*.app, computes size + SHA-256 from
the actual files, and parses each .app package header for its segment count
using the SAME offsets the flasher page validates with:

    bytes 0-3   magic, ASCII "OVL1"
    bytes 4-7   build hash, u32 LE
    byte  8     segment count
    bytes 12-27 app name, ASCII, NUL/space padded

Display names and notes come from the curated tables below, keyed by file
name. A file with no table entry still lands in the catalog (id/name derived
from the file name, empty notes) so dropping a new build into docs/firmware/
or docs/apps/ and re-running this script is all it takes — but add a table
entry so the page can describe it properly.

Usage:  python3 tools/web-flasher/make_catalog.py
Writes: docs/catalog.json   (paths in the manifest are relative to docs/,
which is the GitHub Pages root the flasher page is served from)
"""

import hashlib
import json
import struct
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
DOCS = REPO / "docs"

# Curated display metadata, keyed by file name in docs/firmware/ / docs/apps/.
FIRMWARE_META = {
    "bf5rh-m0.BF": {
        "id": "bf5rh-m0",
        "name": "bf-5rh M0 (upstream, unmodified)",
        "notes": "First-flash build, boots on 5RH10W-V05, all subsystems verified 2026-11-24.",
    },
}

APP_META = {
    "fm.app": {
        "id": "fm",
        "name": "FM",
        "notes": "FM broadcast radio.",
    },
    "settings.app": {
        "id": "settings",
        "name": "SETTINGS",
        "notes": "On-radio settings editor.",
    },
    "spectrum.app": {
        "id": "spectrum",
        "name": "SPECTRUM",
        "notes": "Spectrum sweep display.",
    },
    "chanmgr.app": {
        "id": "chanmgr",
        "name": "CHANMGR",
        "notes": "Channel manager.",
    },
    "satellite.app": {
        "id": "satellite",
        "name": "SATELLITE",
        "notes": "Satellite pass prediction + tracking (needs satellite table + observer position).",
    },
    "search.app": {
        "id": "search",
        "name": "SEARCH",
        "notes": "Frequency search / found-frequency list.",
    },
    "scanqt.app": {
        "id": "scanqt",
        "name": "SCANQT",
        "notes": "CTCSS/DCS tone hunter on the current channel.",
    },
    "actlog.app": {
        "id": "actlog",
        "name": "ACTLOG",
        "notes": "Activity logger: scans programmed channels, logs squelch hits (persistent).",
    },
    "rptbook.app": {
        "id": "rptbook",
        "name": "RPTBOOK",
        "notes": "Repeater book: Preston's programmed channels as a tuneable directory (accepts injected ZIP data).",
    },
    "foxhunt.app": {
        "id": "foxhunt",
        "name": "FOXHUNT",
        "notes": "Direction-finding helper: big RSSI readout, peak hold, bearing samples.",
    },
    "wxpanel.app": {
        "id": "wxpanel",
        "name": "WXPANEL",
        "notes": "NOAA weather panel: 7 WX channels with per-channel RSSI + auto-strongest.",
    },
}


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def app_header(data: bytes) -> dict:
    """Parse an .app header with the page's offsets; fail loudly if invalid."""
    if len(data) < 108:
        raise ValueError(f"only {len(data)} bytes — shorter than the 108-byte header")
    magic = data[0:4].decode("ascii", errors="replace")
    if magic != "OVL1":
        raise ValueError(f"bad magic {magic!r} — expected 'OVL1'")
    build_hash = struct.unpack_from("<I", data, 4)[0]
    segments = data[8]
    name = data[12:28].decode("ascii", errors="replace").strip("\x00 ")
    return {"build_hash": build_hash, "segments": segments, "header_name": name}


def main() -> int:
    firmwares = []
    for path in sorted((DOCS / "firmware").glob("*.BF")):
        data = path.read_bytes()
        meta = FIRMWARE_META.get(path.name, {})
        firmwares.append({
            "id": meta.get("id", path.stem.lower()),
            "name": meta.get("name", path.name),
            "file": f"firmware/{path.name}",
            "size": len(data),
            "sha256": sha256_hex(data),
            "notes": meta.get("notes", ""),
        })

    apps = []
    for path in sorted((DOCS / "apps").glob("*.app")):
        data = path.read_bytes()
        try:
            hdr = app_header(data)
        except ValueError as e:
            print(f"ERROR: {path.name}: {e}", file=sys.stderr)
            return 1
        meta = APP_META.get(path.name, {})
        apps.append({
            "id": meta.get("id", path.stem.lower()),
            "name": meta.get("name", hdr["header_name"] or path.name),
            "file": f"apps/{path.name}",
            "size": len(data),
            "sha256": sha256_hex(data),
            "segments": hdr["segments"],
            "notes": meta.get("notes", ""),
        })

    catalog = {"firmwares": firmwares, "apps": apps}
    out = DOCS / "catalog.json"
    out.write_text(json.dumps(catalog, indent=2) + "\n")
    print(f"Wrote {out}: {len(firmwares)} firmware(s), {len(apps)} app(s)")
    for e in firmwares:
        print(f"  fw  {e['file']:<28} {e['size']:>7} B  sha256 {e['sha256']}")
    for e in apps:
        print(f"  app {e['file']:<28} {e['size']:>7} B  sha256 {e['sha256']}  segments {e['segments']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
