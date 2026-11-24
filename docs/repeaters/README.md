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

## File import — the operator's own export (added 2026-11-24)

Preston's decision: instead of hosted shards, he loads **his own
RepeaterBook export** into the flasher locally. This is exactly the use
RepeaterBook's export terms allow (registered user, personal use, no
redistribution — see the Exports wiki page). The file is read with
FileReader **in the browser only** — never uploaded, fetched anywhere,
or stored on a server — and the parsed records feed the same ZIP +
radius preview and the same verified RPBK block-writing path as the
shard flow (source byte 1). The hosted-shard fetch remains as the
fallback when no file is loaded.

Supported inputs (detected by content/headers, not extension):

| Format | Coordinates? | Notes |
|---|---|---|
| Shard JSON (this directory's format) | Yes | Same schema as the hosted shards; interchangeable |
| RepeaterBook "Web Export" CSV | **No** | Documented columns below |
| CHIRP CSV (incl. RepeaterBook's CHIRP export) | **No** | Location/Name/Frequency/Duplex/Offset/Tone/rToneFreq/cToneFreq/DtcsCode/… |
| RepeaterBook BTECH UV-PRO/GMRS-PRO CSV | **No** | Hz frequencies; sub-audio cells (`15140` = CTCSS 151.4 Hz, `DCS 023N`) |
| RepeaterBook GPX | **Yes** | Waypoint lat/lon attributes; freq/tone/callsign pattern-extracted from name+desc text |
| RepeaterBook KML | **Yes** | Placemark coordinates (lon,lat order); same text extraction as GPX |
| Minimal CSV `name,freq_mhz,tone,lat,lon` | Yes | Hand-built files |

Parsers: header-driven, case-insensitive column aliases; malformed rows
are skipped with a visible count, never fatal. Unit tests + fixtures:
`tools/web-flasher/repeater-import/` (`node test-parsers.mjs`, 46
checks, including a byte-identical RPBK cross-check against
`make_rpbk.py`).

### The coordinates problem — verified findings (2026-11-24)

Checked against RepeaterBook's own public wiki (Exports, FAQ, GPX,
BTECH, API, and Favorites pages, fetched 2026-11-24):

- Exports require a **free registered account, logged in**; personal
  use only; **no redistribution** (Exports page). Preston's model —
  his own export, loaded locally, never hosted — is compliant.
- Export formats offered from search results: CSV, XLS, **KML**,
  **GPX**, plus radio-specific formats (CHIRP, RT Systems, Yaesu,
  Anytone, Icom, Kenwood, G4HFQ, BTECH, TravelPlus).
- The documented **Web Export column set** (API wiki): Channel Number,
  Receive Frequency, Transmit Frequency, Offset Direction (−, +, s, x),
  Name (callsign), Tone Mode (None, Tone, T SQL, DCS, D Code), CTCSS,
  RX CTCSS, DCS, RX DCS, Comment (nearest city and landmark).
  **No latitude/longitude.**
- The CHIRP and BTECH exports likewise carry **no coordinates** — they
  are radio-programming files, not geographic ones.
- **GPX is the coordinate-bearing export**: the GPX wiki page documents
  it as available from Proximity, Location, Quick, Feature, Keyword,
  and Travel searches (North America), with waypoints carrying lat/lon
  plus "frequency, tone, and repeater notes" metadata. KML is likewise
  listed as available on the Exports page. (The Favorites module still
  lists GPX/KML as "In Development" — that is the Favorites export
  list, not search-results export.)
- The exact GPX/KML metadata text layout is **not publicly
  documented**; the parser is deliberately pattern-tolerant (frequency,
  +/− offset sign, PL/CTCSS/DCS patterns, callsign token). Preston's
  first real export file will confirm it; the fixture in
  `tools/web-flasher/repeater-import/fixtures/` is shaped from the
  documented waypoint model, not from a captured real export.
- The API is **not** used: since 2026-03-03 it is restricted to
  approved clients with tokens (API wiki page).

**Decision (documented):** entries without coordinates cannot be
distance-filtered. They are still listed and installable — sorted after
the distance-known entries and marked **"no coords"** in the preview,
with the status line stating how many were included unfiltered. For
the full ZIP + radius experience, export **GPX** (or KML).

### How Preston exports from RepeaterBook (step by step)

1. Go to repeaterbook.com and **log in** (free account; the Export menu
   only appears when signed in).
2. Run a **Proximity search**: Search → Proximity, enter ZIP **28052**
   (Gastonia) and a radius (25–50 mi), pick the bands you want
   (2 m / 70 cm / GMRS), and search.
3. On the results page, open the **Export** menu (on some flows:
   check the rows you want, then **Export Selected**).
4. Choose the format:
   - **GPX** — recommended: carries coordinates, so the flasher's
     ZIP + radius filter works fully.
   - **CSV** or **CHIRP** — also load fine, but carry no coordinates;
     entries will be listed unfiltered ("no coords").
5. Save the downloaded file.
6. In the web flasher, open the **Apps** hub → **RPTBOOK** panel →
   **"Load repeater data from a file"** → pick the file. The panel
   shows what was parsed (count, how many have coordinates, rows
   skipped).
7. Enter ZIP **28052** + radius → **Look up repeaters** → check the
   preview → **Install** writes the app + these repeaters, verified by
   read-back as usual.

(Menu labels follow RepeaterBook's current wiki documentation; their
site occasionally varies the exact wording by search type.)

## Satellite observer position — no typing (added 2026-11-24)

The SATELLITE panel's observer lat/lon can now be filled three ways
(the settings write path itself is unchanged — operator still presses
**Save position**, which read-modify-writes settings bytes 32–39 and
verifies by read-back):

- **Use my current location** — browser geolocation
  (`navigator.geolocation`); denial/unavailability is reported plainly.
- **ZIP lookup** — zippopotam.us, same service as the RPTBOOK flow.
- **Address lookup** — OpenStreetMap Nominatim, one user-triggered
  request per click (no batching/autocomplete, per their usage
  policy); first result wins, no-result reported plainly.

Each fill shows the resolved place name when available, and the last
position is persisted in the browser's localStorage
(`ar5rm.satpos`) for prefill on the next visit (prefill order: radio's
stored position → localStorage → this session's ZIP geocode).
