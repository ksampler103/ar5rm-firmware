# Repeater file-import parser tests

The web flasher's RPTBOOK panel imports repeater files the operator
exported himself (RepeaterBook CSV/GPX/KML, CHIRP CSV, BTECH CSV,
minimal CSV, shard JSON). The parsers live in the flasher page between
`RPTIMPORT-BEGIN` / `RPTIMPORT-END` markers as pure functions;
`test-parsers.mjs` extracts them from the page source and runs them
against the fixtures here, then verifies the parsed records feed the
page's `buildRpbkBlock()` to a block that is byte-identical to
`tools/simulator/tools/make_rpbk.py` output (except header source
byte 5: page writes 1, the Python builder writes 0).

Run:

```
node tools/web-flasher/repeater-import/test-parsers.mjs
```

Page source resolution: `$FLASHER_HTML`, else the artifact source at
`~/workspace/ts-spaces/ar-5rm-web-flasher/index.html`, else the repo's
`docs/index.html` (lags until the coordinator syncs Pages).

Fixtures are synthetic Gastonia-area data shaped to each format's
documented columns — they are parser test vectors, not a repeater
dataset. Design and licensing notes: `docs/repeaters/README.md`.
