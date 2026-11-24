# Web flasher redesign — progressive disclosure, detection, hub, app picker

2026-11-24. UI/information-architecture rebuild on top of the existing,
hardware-proven protocol code. The protocol layer did not change: a
line-level diff of the page script against the pre-redesign page shows the
only modified pre-existing lines are state-machine wiring (listed at the
bottom); every protocol function body (backup, probe, flash framing, CPS
transact/settings/band-lock/install, .BF/.app validation) is byte-identical.

## New flow / state machine

At load, only **Step 0 (Connect)** and the **log pane** are visible. A
visibility manager (`showSections`) shows exactly one stage at a time from:
`secDetect, secBackup, secProbe, secFlash, secHub, secCps, secApps,
secFlashed`. Flow state lives in `S.flowStage` / `S.view` / `S.fromHub` /
`S.detected` / `S.backupWaived`.

- **Connect** → on success, detection runs automatically (also available
  any time via the **Detect radio** button in Step 0). A persistent chip in
  Step 0 shows the result: `STOCK` / `CUSTOM — bf-5rh` / `FLASH MODE` /
  `not detected`, with `(manual)` appended for overrides.
- **STOCK flow (enforced order):** Backup card only → backup success →
  Probe card only → BOOTLOADER ANSWERED → Flash card → flash success →
  "Your radio now runs custom firmware" card → **Re-detect radio** →
  detection now finds bf-5rh → optional-backup stage → hub. The skip
  button does not exist in this flow; the flash gates are unchanged
  (a backup from earlier in the same session still counts — re-detection
  then skips straight to Probe).
- **CUSTOM flow:** Backup card marked **OPTIONAL** with a **Skip backup**
  button (either path proceeds) → **HUB** with three tiles, one section
  visible at a time, each with a **← Back to hub** affordance:
  - **Upgrade** — explains the power-cycle into flash mode, then the
    existing Probe card and Flash card (catalog/upload picker, SHA gates,
    confirm modal — all unchanged).
  - **CPS** — the previous Step 4 CPS parts (handshake, settings read,
    band lock), unchanged implementations, plus a visible note that full
    programming (all settings, channel editor) arrives in the next CPS
    phase. The full editor was NOT built here.
  - **Apps** — the graphical picker (below), with its own CPS
    connect/read buttons driving the same shared CPS session.
- **FLASH MODE entry:** detection found the bootloader (see below). The
  detection card offers **Continue to firmware flash** — which waives the
  backup gate (`S.backupWaived`, gate label updated to say so, and a risk
  box appears in the Flash card: no backup exists because the radio was
  already in its bootloader) — or **Power on normally instead**, which
  returns to Step 0 for a normal power-on + re-detect.
- **No radio:** detection card with the troubleshooting checklist
  (radio ON at main screen, cable seated hard, correct port, Windows
  Chrome/Edge, no other program holding the cable), **Retry detection**,
  and two manual overrides (**My radio is stock** / **My radio runs
  bf-5rh**) that enter the respective flows with the chip marked manual.
  Never a dead end.
- Disconnect (button or cable unplug) resets the flow to Step 0 only;
  session facts the old page kept (backup/probe done) still persist, as
  before.

Flow advancement is hooked by wrapping the existing button handlers:
`btnBackup → doBackup().then(afterBackup)`, `btnProbe → …afterProbe`,
`btnConfirmFlash → doFlash().then(afterFlash)` (success detected from the
flash pill reaching "complete"), `btnConnect → doConnect().then(…
doDetect())`, `btnDisconnect → doDisconnect().then(resetFlow)`.

## Detection sequence and rationale

Runs immediately after a successful Connect, single pass, in this order:

1. **bf-5rh CPS handshake** — send `PROGRAMBF-5RH`, wait up to **1.0 s**
   for `0x06` (existing CPS machinery: `waitForCpsAck`). ACK ⇒ **CUSTOM**.
   The session is then closed cleanly with the CPS **END** frame
   (`cpsTransact(CPS_CMD.END, …)`) so the radio doesn't sit in CPS mode
   and reboot on its ~2 s idle timeout; an un-ACKed END is logged as a
   warning (the idle timeout still exits CPS).
2. **Stock ident** — send `PROGRAMBFNORMALU`, wait up to **2 s** for
   `0x06`, **one attempt only** (this is detection, not the backup; the
   backup's own CHIRP-style ident phase with retries runs when the Backup
   card is used). ACK ⇒ **STOCK**; the radio is now in its program mode
   and the enforced Backup follows naturally.
3. **Bootloader probe** — the existing handshake, up to 3 attempts
   (`detectProbeBootloader`, a read-only clone of `doProbe`'s loop that
   returns a boolean instead of driving the Probe card). ANSWERED ⇒
   **FLASH MODE**; detection also satisfies the probe gate
   (`S.probeOk = true`).
4. Nothing ⇒ **no-radio card** (checklist + retry + manual overrides).

**Why custom is tried before stock:** a bf-5rh radio left sitting in CPS
mode reboots on its ~2 s idle timeout, so the CPS probe must go first and
be closed out immediately; a custom radio must also never be misread as
stock and pushed through the forced stock backup→flash path. The stock
ident is harmless to a bf-5rh radio (it only ACKs its own ident), and the
bootloader speaks neither ident, so the order is safe for all three
states. Known interaction to watch on hardware: after step 2 ACKs, the
stock radio sits in program mode; the Backup card's ident phase re-sends
the ident with CHIRP-style retries — if a live backup's first ident
attempt fails, power-cycle the radio and use Retry detection.

**Live detection against hardware is unverified** — Preston's run is the
test, especially the detection ordering and the clean END after a custom
detection.

## Graphical app picker (Apps tile)

The picker **is** the catalog view: one tile per catalog app (inline SVG
icon + name). Clicking a tile fills a detail panel — full name, blurb,
usage line, size, segment count, and 1–2 simulator screenshots referenced
as relative `appimg/<file>` URLs (each `<img>` hides itself on error, so
a missing file never shows a broken-image icon) — and runs the fetched
bytes through the same SHA-256 manifest check + `loadCpsApp` header
validation (incl. `EXPECTED_BUILD_HASH`) as before. Install controls
(slot picker + Install) are the existing implementation, moved into the
Apps section with their element IDs unchanged. **Upload a .app file**
remains as a secondary source option. The firmware picker in Flash keeps
its catalog/upload selector unchanged.

Per-app picker metadata (icon, blurb, usage, screenshots) lives in
`make_catalog.py` `APP_META` and is emitted into `catalog.json`; icons
are drawn by the page's `APP_ICONS` table (generic puzzle-style fallback
for unknown ids). Screenshots are simulator frames staged in this repo
under `assets/appimg/` (640×512, from `tools/simulator/examples/` and the
build box's `~/simwork/sim/runs/`).

## Verification performed

- `node --check` on the extracted page script: OK.
- Script diff vs the pre-redesign page: 28 old-side lines changed, all
  wiring (below); all protocol function bodies byte-identical.
- DOM-stub harness (Node `vm`, elements seeded from the real HTML,
  protocol entry points stubbed per scenario): 45/45 checks pass —
  (a) initial render = Step 0 + log only; (b) simulated custom detection
  → optional backup → hub → all three tiles + back; (c) simulated stock
  detection enforces backup→probe→flash with the hub unreachable;
  (d) catalog failure falls back to upload in both pickers; (e) app tile
  selection populates the detail panel and validates the real fm.app
  bytes through the page's own header parser; (f) flash-mode detection →
  waived gate → flash; (g) no-radio card + manual override.
- `make_catalog.py` run in a scratch copy: emits all 11 apps with
  icon/blurb/usage/screenshots; the real `docs/catalog.json` was NOT
  regenerated by this task (coordinator step, below).

## Coordinator sync checklist (Pages)

The artifact (`ar-5rm-web-flasher`) and the standalone export
(`~/workspace/your_files/ar-5rm-web-flasher/ar-5rm-web-flasher.html`) are
already updated. For GitHub Pages, after the theme build's additions land:

1. Copy the redesigned page over `docs/index.html` (from the artifact
   export above).
2. Copy `assets/appimg/*.png` (16 files) → `docs/appimg/`.
3. Re-run `python3 tools/web-flasher/make_catalog.py` to regenerate
   `docs/catalog.json` — must happen AFTER the theme build's
   firmware/app files are in `docs/`, so the fork firmware + THEMES app
   land in the same manifest with their picker metadata (a `themes.app`
   APP_META entry already exists; it emits only if the file is present).
4. Commit `docs/` and push; verify the Pages copy shows Step 0 + log
   only on load, and that an app tile's screenshots load from
   `appimg/`.

## Wiring exceptions to the no-regression rule (complete list)

Presentation/state wiring only — no protocol behavior changed:

- `S` gains flow fields (`detected`, `backupWaived`, `flowStage`,
  `view`, `fromHub`, `selAppId`).
- `refreshControls`: dropped the removed app-`<select>` line; added
  enable/disable for the new detection buttons and the Apps-tile CPS
  controls + its CPS pill.
- `allGatesMet` / `updateGates`: backup gate accepts
  `S.backupDone || S.backupWaived` (waiver only settable from the
  flash-mode detection card); gate label text reflects the waiver.
- `loadCatalog`: builds app tiles (`buildAppTiles`) instead of filling
  the removed `<select>`; firmware side unchanged.
- `onCpsAppSourceChange`: resets picker selection/detail instead of the
  removed `<select>`/details elements.
- `onCpsAppCatalogChosen` removed; its fetch + SHA-256 + `loadCpsApp`
  logic moved verbatim into `loadCatalogApp` (called by tile clicks).
- Button wiring wraps `doConnect/doDisconnect/doBackup/doProbe/doFlash`
  with `.then(...)` flow hooks; the serial-unplug handler also calls
  `resetFlow()`. The CPS install sub-section DOM moved from the CPS
  section into the new Apps section with all element IDs preserved, so
  `doCpsInstall`/`loadCpsApp`/`renderCpsSlotOptions` are untouched.
- New additive code: detection (`doDetect`, `detectProbeBootloader`),
  flow functions, and the picker (`APP_ICONS`, `buildAppTiles`,
  `selectAppTile`, `renderAppDetail`, `loadCatalogApp`).
