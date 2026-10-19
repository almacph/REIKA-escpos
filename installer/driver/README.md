# WinUSB package inputs

The supported inventory is limited to `0483:5840` (Standard/XP-58IIH) and
`0FE6:811E` (ICS Advent). The `interface` values in
`supported-devices.toml` remain `CAPTURE_REQUIRED`: a physical composite-device
capture must record the real interface and compatible hardware IDs before an
INF is authored. This repository does not claim that a VID/PID match alone is
printer-ready.

Production setup requires a device-specific INF and CAT signed by the release
owner under the Windows driver-signing policy. A locally generated or
self-signed catalog, test-signing mode, or an unreviewed Zadig package cannot
be shipped as the integrated installer. The elevated helper and binding
transaction are later package work; this group only supplies the reviewed
allowlist and read-only diagnostics.

The helper accepts a package only when the fixed INF/CAT files and a
`signing-provenance.json` record containing the trusted catalog marker are
present. The checked-in `.inf.template` files intentionally contain a capture
placeholder and cannot pass that gate.
