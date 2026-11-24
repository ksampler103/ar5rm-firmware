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

App entries also carry the graphical-picker fields (redesign, 2026-11-24):
"icon" (an APP_ICONS key in the flasher page), "blurb" (2-3 sentence
description), "usage" (one-line how-to), and "screenshots" (relative
"appimg/<file>" paths — simulator frames staged under assets/appimg/ in
this repo and copied to docs/appimg/ for Pages; see REDESIGN.md).

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
    "bf5rh-fork1-apx.BF": {
        "id": "bf5rh-fork1-apx",
        "name": "Fork 1 — APX Pro theme",
        "notes": "bf-5rh + slimmed core with the APX Pro standby theme compiled in. Switching themes = flash a different Fork 1 variant (Upgrade flow). All catalog apps compatible (build hash unchanged).",
    },
    "bf5rh-fork1-clean.BF": {
        "id": "bf5rh-fork1-clean",
        "name": "Fork 1 — Clean Modern theme",
        "notes": "bf-5rh + slimmed core with the Clean Modern standby theme compiled in. Switching themes = flash a different Fork 1 variant (Upgrade flow). All catalog apps compatible (build hash unchanged).",
    },
    "bf5rh-fork1-dense.BF": {
        "id": "bf5rh-fork1-dense",
        "name": "Fork 1 — Data Dense theme",
        "notes": "bf-5rh + slimmed core with the Data Dense standby theme compiled in. Switching themes = flash a different Fork 1 variant (Upgrade flow). All catalog apps compatible (build hash unchanged).",
    },
    "bf5rh-fork1-minimal.BF": {
        "id": "bf5rh-fork1-minimal",
        "name": "Fork 1 — Minimal Dark theme",
        "notes": "bf-5rh + slimmed core with the Minimal Dark standby theme compiled in. Switching themes = flash a different Fork 1 variant (Upgrade flow). All catalog apps compatible (build hash unchanged).",
    },
}

APP_META = {
    "fm.app": {
        "id": "fm",
        "name": "FM",
        "notes": "FM broadcast radio.",
        "icon": "fm",
        "blurb": "FM broadcast receiver for the 87.5\u2013108 MHz band, using the radio's dedicated FM tuner chip. Tune by hand, step through the band, or seek to the next station; up to 30 FM presets are kept in the radio's FM channel list.",
        "usage": "Open FM from the launcher, then use the arrow keys to tune/step and MENU to seek.",
        "screenshots": ["appimg/fm-1.png"],
    },
    "settings.app": {
        "id": "settings",
        "name": "SETTINGS",
        "notes": "On-radio settings editor.",
        "icon": "settings",
        "blurb": "On-radio editor for the firmware's settings, in three groups (menu, radio, system): squelch, band lock, backlight, side-key functions, boot text and more \u2014 the same 168-byte settings record the web CPS reads and writes.",
        "usage": "Pick a group, scroll to a setting, and edit it in place; changes are written to the radio's settings storage.",
        "screenshots": ["appimg/settings-1.png", "appimg/settings-2.png"],
    },
    "spectrum.app": {
        "id": "spectrum",
        "name": "SPECTRUM",
        "notes": "Spectrum sweep display.",
        "icon": "spectrum",
        "blurb": "Live spectrum sweep around the current frequency: samples signal strength across a span and draws it as a bar graph, so you can see band activity, find busy spots, and spot noise at a glance.",
        "usage": "Tune near the band you care about, open SPECTRUM, and watch the sweep; stronger signals draw taller bars.",
        "screenshots": ["appimg/spectrum-1.png"],
    },
    "chanmgr.app": {
        "id": "chanmgr",
        "name": "CHANMGR",
        "notes": "Channel manager.",
        "icon": "chanmgr",
        "blurb": "Channel manager for the radio's programmed memory channels: browse the channel list with names and frequencies, and manage entries without a computer.",
        "usage": "Scroll the list to review channels; follow the on-screen keys to edit or clear an entry.",
        "screenshots": ["appimg/chanmgr-1.png"],
    },
    "satellite.app": {
        "id": "satellite",
        "name": "SATELLITE",
        "notes": "Satellite pass prediction + tracking (needs satellite table + observer position).",
        "icon": "satellite",
        "blurb": "Amateur satellite helper in three parts: a satellite list, pass prediction, and a tracking view with Doppler-aware tuning. It needs its satellite table loaded into the app-data region and an observer position in settings before it can predict passes \u2014 until then it opens to an empty list.",
        "usage": "After the table and observer position are set, pick a satellite to see its next passes and open the track view during a pass.",
        "screenshots": ["appimg/satellite-1.png"],
    },
    "search.app": {
        "id": "search",
        "name": "SEARCH",
        "notes": "Frequency search / found-frequency list.",
        "icon": "search",
        "blurb": "Frequency search: sweeps a range and collects the frequencies where the squelch opens into a found list you can step through and tune.",
        "usage": "Start a search over the range you care about, then step through the found frequencies.",
        "screenshots": ["appimg/search-1.png"],
    },
    "scanqt.app": {
        "id": "scanqt",
        "name": "SCANQT",
        "notes": "CTCSS/DCS tone hunter on the current channel.",
        "icon": "scanqt",
        "blurb": "Tone hunter: sits on the current channel and works out which CTCSS/DCS tone an incoming signal is using, so you can program the right tone for a repeater or simplex contact.",
        "usage": "Tune to an active signal, open SCANQT, and let it test tones while the other station transmits.",
        "screenshots": ["appimg/scanqt-1.png"],
    },
    "actlog.app": {
        "id": "actlog",
        "name": "ACTLOG",
        "notes": "Activity logger: scans programmed channels, logs squelch hits (persistent).",
        "icon": "actlog",
        "blurb": "Unattended activity logger. It scans the radio's programmed channels (about 90 ms per channel, speaker muted) and records every squelch hit \u2014 frequency, start time, peak RSSI, duration \u2014 to flash memory, where the log survives app exit and power cycles (up to 768 hits).",
        "usage": "Leave it running on a shelf or in the car; open the review view (MENU) later to see what was active and when. * + MENU clears the log.",
        "screenshots": ["appimg/actlog-1.png", "appimg/actlog-2.png"],
    },
    "rptbook.app": {
        "id": "rptbook",
        "name": "RPTBOOK",
        "notes": "Repeater book: Preston's programmed channels as a tuneable directory (accepts injected ZIP data).",
        "icon": "rptbook",
        "blurb": "A tuneable repeater directory baked from Preston's own programmed channels (165 entries, parsed from his pre-flash backup): names, frequencies and tones, read-only. It also accepts an injected data block (the ZIP-code flow), so a future web page can load repeaters near any ZIP without rebuilding the app.",
        "usage": "Scroll to an entry and press MENU to tune it exactly in-app; EXIT after a tune lands the standby VFO on the entry's RX frequency.",
        "screenshots": ["appimg/rptbook-1.png", "appimg/rptbook-2.png"],
    },
    "foxhunt.app": {
        "id": "foxhunt",
        "name": "FOXHUNT",
        "notes": "Direction-finding helper: big RSSI readout, peak hold, bearing samples.",
        "icon": "foxhunt",
        "blurb": "Direction-finding helper for hunting a transmitter on foot: a big live RSSI readout in dBm with a bar graph and decaying peak marker, plain-language guidance (STRONG/WARM/WEAK with body-block and attenuation prompts), and up to 8 bearing samples you can snapshot as you move.",
        "usage": "Walk the area watching the readout and peak; press MENU at each bearing point to save a sample, and # toggles a beeper whose rate rises with signal strength.",
        "screenshots": ["appimg/foxhunt-1.png", "appimg/foxhunt-2.png"],
    },
    "wxpanel.app": {
        "id": "wxpanel",
        "name": "WXPANEL",
        "notes": "NOAA weather panel: 7 WX channels with per-channel RSSI + auto-strongest.",
        "icon": "wxpanel",
        "blurb": "NOAA weather radio panel: lists all seven WX channels (162.400\u2013162.550 MHz) with a measured signal strength for each, and automatically tunes the strongest one \u2014 handy for checking which weather transmitter covers your spot.",
        "usage": "Open WXPANEL and it samples all seven channels and lands on the strongest; use UP/DOWN + MENU to tune manually, * re-runs the auto-scan.",
        "screenshots": ["appimg/wxpanel-1.png"],
    },
    "themes.app": {
        "id": "themes",
        "name": "THEMES",
        "notes": "Theme picker for the fork firmware's standby screen.",
        "icon": "themes",
        "blurb": "Theme picker for the fork firmware's standby screen: switch between the APX Pro, Clean Modern, and Data Dense layouts. It writes the theme block the fork core reads at boot, so it only has an effect on a radio running the theme fork firmware.",
        "usage": "Pick a theme from the list; exit to standby (or power-cycle) to see the new standby layout.",
        "screenshots": [],
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
            "icon": meta.get("icon", "generic"),
            "blurb": meta.get("blurb", ""),
            "usage": meta.get("usage", ""),
            "screenshots": meta.get("screenshots", []),
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
