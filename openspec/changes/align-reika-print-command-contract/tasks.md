## 1. Contract and input validation

- [x] 1.1 Capture REIKA-derived fixtures for current commands, both MaxiCode spellings, legacy Italic, CashDrawerPulse, and BitImage; record the consumer revision and confirm the proposed raster limits against diagnostic payloads.
- [x] 1.2 Add pulse/raster models and the MaxiCode alias; preserve the legacy Italic parser and existing desktop serialization compatibility.
- [x] 1.3 Add whole-job preflight with bounded body/base64/dimensions/length arithmetic and deterministic parameter validation; ensure input errors never enter USB retry.

## 2. Execution, preview, and reprints

- [x] 2.1 Replace unstaged active italic behavior with a no-op in normal and reprint execution; verify no italic-on or italic-off command bytes are generated.
- [x] 2.2 Implement pulse bytes and raster bands through shared normal/reprint execution semantics; verify legacy CashDrawer is unchanged.
- [x] 2.3 Count BitImage as visible content for midpoint markers; remove italic reset/restore effects while preserving other formatting and the three marker positions.
- [x] 2.4 Render raster content in desktop preview and ignore legacy Italic; verify existing persisted logs remain readable and reprints remain absent from history.

## 3. Contract verification

- [x] 3.1 Test byte output for each pin/duration, both MaxiCode names, raster row padding and 24-row band boundaries, and legacy italic mixed with other styles.
- [x] 3.2 Test both HTTP endpoints for valid fixtures, malformed JSON, unknown commands, invalid parameters, size limits, and zero USB writes for rejected jobs; keep standard snake_case responses and CORS behavior.
- [x] 3.3 Test GUI reprint validation and marker placement with image-only, mixed, and legacy receipts; confirm recoverable USB errors still retry while input errors terminate.
- [ ] 3.4 Run REIKA diagnostics against a Windows build for both supported presets, including drawer delivery and raster/2D output; record unsupported physical features separately from transport success.

## 4. Documentation and completion

- [x] 4.1 Reconcile README, project context, command counts, endpoint references, and schema-package guidance with the approved contract; preserve unrelated unstaged formatting changes.
- [x] 4.2 Restore missing Purpose sections in escpos-commands and usb-driver while preserving their requirements; reconcile the existing direct italic base-spec edit during apply/archive.
- [x] 4.3 Run appropriate Rust format/build/tests and strict OpenSpec validation; record Windows-only checks that cannot run locally.
- [ ] 4.4 After implementation and deployment, archive the proposal and merge its deltas into base specs; do not mark the pre-existing unstaged work complete without verification.
