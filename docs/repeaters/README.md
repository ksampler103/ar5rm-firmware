# Repeater shards for the RPTBOOK ZIP flow

The web flasher's RPTBOOK panel geocodes a US ZIP (zippopotam.us),
fetches ONE shard from this directory — `repeaters/<zip3>.json`, where
`<zip3>` is the first 3 digits of the ZIP — filters it by radius, shows
a preview, and writes the matching entries to the radio as an RPBK
block (see `tools/web-flasher/APPS.md` for the block contract).

## Shard format

```json
{
  "zip3": "280",
  "source": "<dataset name + license>",
  "generated": "<ISO date>",
  "repeaters": [
    {"name": "GASTONIA 600", "rx_hz": 462600000, "tx_hz": 467600000,
     "tx_tone": 1413, "rx_tone": 0, "lat": 35.2621, "lon": -81.1783}
  ]
}
```

- `name`: ≤ 16 chars (truncated into the RPBK record).
- `rx_hz` / `tx_hz`: Hz; `tx_hz` = `rx_hz` (or 0) for simplex.
- `tx_tone` / `rx_tone`: firmware subaudio encoding (0 = none,
  1–105 DCS normal, 106–250 DCS inverted, >250 = CTCSS tenths of Hz).
- `lat` / `lon`: decimal degrees of the repeater site (distance and
  radius filtering are computed against these).

## Dataset status (2026-11-24): NO SHARDS SHIPPED — licensing

The flow is fully implemented, but no production shard data ships yet,
because no lawful, openly usable dataset with usable North/South
Carolina coverage was found. Survey, with evidence:

| Source | License / terms | NC/SC coverage | Verdict |
|---|---|---|---|
| RepeaterBook | Their wiki (current 2026 terms): bulk extraction, mirroring, redistribution, and offline bundling require **written permission**; API restricted to approved clients since 2026-03-03 | Excellent (incl. GMRS) | **Barred without written permission** |
| OpenStreetMap | ODbL (lawful, attribution) | Effectively zero amateur repeaters mapped near Gastonia, NC (Overpass: 0 `communication:amateur_radio` elements within 40 km of 35.262, −81.178) | Lawful but empty here |
| ARD-RepeaterList (GitHub, CC0-1.0) | CC0; project policy explicitly forbids copying from RepeaterBook/Artsci — clean provenance | 7,419 entries in ~20 states, **zero NC and zero SC entries** (checked 2026-11-24) | Lawful, no local data yet |
| hearham.com | No open data license published; premium tier sold | Has NC listings | Not redistributable |
| repeater.world | Code AGPL-3.0, "free data" claim; no data dump or explicit data license found | Unverified | Not usable as-is |
| ATAK NC repeater package (getgotak) | Commercial product ($13/state) | NC statewide | Not open |

## Paths to real data (Preston's call)

1. **RepeaterBook written permission** — their wiki says non-commercial
   API access may be approved at no cost, but bundling shards here is
   redistribution/offline bundling, which needs explicit written
   permission. Highest-quality data (ham + GMRS).
2. **ARD (CC0)** — adopt when NC/SC land; contributing verified local
   entries (from Preston's own knowledge / repeater-owner coordination,
   per ARD's policy) would make the local shard lawful immediately for
   the entries he can vouch for.
3. **Hand-built local shard** — a small `280.json` built only from
   repeaters Preston personally knows and can verify (his own data is
   his to bundle). The no-ZIP default already ships his 165 programmed
   channels via `docs/appdata/rptbook-default.bin`; a ZIP shard needs
   site coordinates, which his channel list does not carry.

Until one of these lands, entering a ZIP in the panel honestly reports
that no repeater data is available for the area yet; installing without
a ZIP writes the owner-default block (Preston's own 165 channels).
