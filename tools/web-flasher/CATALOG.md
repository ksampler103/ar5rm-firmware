# Built-in catalog (web flasher)

The flasher page (`docs/index.html`, served by GitHub Pages from `docs/`)
offers firmwares and overlay apps from a built-in catalog, served by the
site itself, alongside the manual file-upload path (which always remains).

## Layout

```
docs/
  index.html          the flasher page
  catalog.json        manifest the page fetches at load (relative URL)
  firmware/*.BF       built-in firmware packages
  apps/*.app          built-in overlay app packages
```

The page fetches `catalog.json` relative to its own URL, populates the
Step 3 (firmware) and Step 4 (app) dropdowns from it, and on selection
fetches the entry's file, **verifies its SHA-256 against the manifest**,
and only then runs the exact same validation/gates/install flow as an
uploaded file. If `catalog.json` can't be loaded (e.g. a copy of the page
hosted somewhere without the catalog files), built-in mode disables itself
with a note and upload still works.

## Adding a firmware or app

1. Build the package as usual (firmware: see `PACKAGE.md`; apps: see
   `APPS.md` — apps must be built against the current firmware ABI or the
   page's build-hash validation will reject them).
2. Copy the file into `docs/firmware/` (`.BF`) or `docs/apps/` (`.app`).
3. Add a display entry for the file name in `make_catalog.py`
   (`FIRMWARE_META` / `APP_META`: id, name, one-line notes) so the page can
   describe it properly. Files without an entry still appear, with a
   derived name and no notes.
4. Regenerate the manifest:

   ```
   python3 tools/web-flasher/make_catalog.py
   ```

   It scans `docs/firmware/*.BF` and `docs/apps/*.app`, computes sizes and
   SHA-256 from the files themselves, and parses each app header (magic /
   segment count, same offsets the page validates with). Never hand-edit
   hashes in `catalog.json` — regenerate.
5. Commit `docs/catalog.json`, the new file under `docs/firmware/` or
   `docs/apps/`, and any `make_catalog.py` metadata change together.
   GitHub Pages picks it up on its next rebuild.

## Notes

- The M0 firmware (`docs/firmware/bf5rh-m0.BF`) is our own GPL build output
  (upstream bf-5rh, unmodified) — the same package recorded in `PACKAGE.md`
  (SHA-256 `b04356cb…c90357e`). Hosting it is Preston's explicit choice
  (2026-11-24) so the page can offer it built-in. Never add stock factory
  images here — the project rule against committing stock firmware stands.
- Manifest `file` paths are relative to `docs/` (`firmware/<name>`,
  `apps/<name>`); the page resolves them against its own URL, so forks and
  renames keep working.
