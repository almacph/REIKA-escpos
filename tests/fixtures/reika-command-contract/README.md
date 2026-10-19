# REIKA command contract fixtures

These fixtures are captured from the REIKA consumer at commit `0e5fcc1f` and
the desktop compatibility baseline described in
`openspec/changes/align-reika-print-command-contract/evidence.md`.

The proposed service limits are: 4 MiB maximum JSON body, raster width 1–576
pixels, height 1–4096 pixels, and at most 294,912 decoded raster bytes. Raster
rows use `ceil(width / 8)` bytes and the consumer sends normal `GS v 0` bands
of at most 24 rows.

The `Maxicode` spelling is the consumer spelling. `MaxiCode` remains the
desktop serialization spelling for persisted payload compatibility. `Italic`
is retained as a legacy parser value. `CashDrawer` remains the legacy drawer
command; `CashDrawerPulse` is the explicit duration form.
