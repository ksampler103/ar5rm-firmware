# Versioning

This project versions its firmware as **vMAJOR.MINOR.PATCH** (at most two
decimal points). The version applies to the firmware built from this fork —
the flashable `.BF` packages — not to individual overlay apps or the web
flasher page.

## What each number means

- **PATCH** — slimming, bug fixes, rebuilds: no user-visible feature change.
  Example: a core diet that changes no behavior is a patch bump (v1.0.0 →
  v1.0.1).
- **MINOR** — a new user-visible feature or capability: a new theme variant
  set, the graphical launcher, a new core feature.
- **MAJOR** — a breaking change: an ABI / BUILD_HASH change that invalidates
  existing overlay apps, or a new fork generation.

## Rules

- Every firmware-affecting merge to `main` bumps the version, adds a
  `CHANGELOG.md` entry, tags the commit, and gets a GitHub release whose
  assets are the flashable `.BF` files for that version, with SHA-256s in
  the release notes.
- App-only changes (overlay apps, the web flasher page) do **not** bump the
  firmware version. They are noted in the changelog's "Apps & tools" area
  of the next firmware release if relevant.

## Release procedure

1. Land the firmware change on `main`.
2. Decide the bump (PATCH / MINOR / MAJOR per the rules above).
3. Add the `CHANGELOG.md` entry (newest first): date, what changed, sizes,
   SHA-256s of the `.BF` packages.
4. Tag the source commit: `git tag -a vX.Y.Z -m "..."` and push the tag.
5. Create the GitHub release from that tag with the version's `.BF` files
   attached as assets and the SHA-256 table in the notes (release body is
   drafted in `releases/vX.Y.Z-notes.md`).
6. Update the "current version" line below.

## Current version

**v1.0.0** — Fork 1: slimmed core + theme variants (2026-11-24).
