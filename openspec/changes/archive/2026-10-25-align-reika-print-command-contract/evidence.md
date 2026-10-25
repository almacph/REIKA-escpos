# Spec gap review — 2026-10-18

## Sources and scope

Read local source and specs, without fetching or changing sibling repositories. Desktop HEAD: `a687806`; REIKA HEAD: `0e5fcc1f` (only an unrelated untracked unit-test directory was listed); `REIKA-printer-schemas` HEAD: `2b32cec` (clean). Inspected REIKA-senri's README: it is a vehicle-detection API, with no printer contract needed for this proposal. No basis was found to expand into OCR, voucher hosting, or other unrelated application behavior.

## Findings

| Priority | Evidence | Gap / disposition |
|---|---|---|
| High | `../REIKA/openspec/specs/print-transport/spec.md`, requirement “Thermal output uses regular text”; `../REIKA/src/lib/printer/escposEncoder.ts` case `Italic` | Current upstream requires no-op legacy italics; unstaged Rust emits `1B 34` / `1B 35`, restores italics on reprints, and renders slanted preview text. Resolve in this proposal. |
| High | `../REIKA/src/lib/model/schemas/receiptSchema.ts`; `../REIKA/src/lib/printer/testReceipt.ts`; `../REIKA/src/lib/printer/escposEncoder.ts`; `src/models/command.rs` | `CashDrawerPulse` exists upstream but is unknown to Rust. Add explicit pulse semantics and malformed-input cases. Legacy drawer kick still uses `CashDrawer`, so not every drawer action is currently broken. |
| High | Same schema/encoder; `../REIKA/src/routes/diagnose/PrinterTest.svelte` raster payload; Rust enum | `BitImage` can reach HTTP and is rejected. Add bounded raster input, byte fixtures, preview, and reprint content classification. |
| Medium | Consumer schema/diagnostic `Maxicode` versus Rust `MaxiCode` | Add alias without removing the saved desktop spelling. |
| High | `src/main.rs` starts `run_with_port` only after `try_open` succeeds | Existing watchdog proposal needs no-printer cold-start coverage; merely adding a route will not make liveness printer-independent. Add scenario and prerequisite task there. |
| Medium | `src/handlers/print.rs`, `src/routes/print.rs`, `src/services/printer.rs::with_retry` | Typed JSON errors are handled, malformed JSON is rejected before handlers, and execution retries every PrinterError. Specify uniform JSON validation errors and validate before USB; invalid input must not retry forever. |
| Medium | `../REIKA-printer-schemas/src/schema.ts` | Old package uses `isConnected`, `testLine`, `testPage`; live service and consumer use snake_case. Document fixture authority; do not migrate to the package as-is. |
| Medium | `openspec validate --all --strict --no-interactive` | Existing `escpos-commands` and `usb-driver` lack `## Purpose`. Five items pass and two fail before this proposal. Schedule structural repair; preserve current user edits now. |
| Low | `openspec/project.md`, command reference table, HTTP endpoint reference | Stale 40-command count and endpoint summaries omit newer behavior. Reconcile during implementation/archive, using actual enum inventory. |

## Complete unstaged inventory

Initial diff: 13 tracked files, 230 insertions and 76 deletions; no staged changes reported. Existing untracked package: `add-windows-setup-and-watchdog` (17 unchecked tasks before this review).

| Paths | Classification and proposal treatment |
|---|---|
| `src/models/command.rs` | Adds `Italic(bool)` and a JSON round-trip test. Retain compatibility parsing; task completion remains unverified. |
| `src/services/printer.rs` | Adds italic constants, both execution branches, and command-name tracing. Proposed output must be no-op rather than emitting these constants. |
| `src/services/reprint.rs` | Adds italic tracking/reset/restore plus tests and formatting. Proposed behavior ignores legacy italic transitions while preserving all other marker state. |
| `src/app/receipt_renderer.rs` | Adds italic state and rendering plus formatting. Proposed preview must remain regular text. |
| `README.md`, `openspec/specs/escpos-commands/spec.md` | Advertise active Italic and update the table to 41. Preserve existing edits; reconcile to approved compatibility semantics at apply/archive. |
| `src/app/gui.rs`, `src/app/mod.rs`, `src/app/print_log.rs`, `src/app/single_instance.rs`, `src/app/tray.rs`, `src/services/sensor_reporter.rs`, `src/services/usb_driver.rs` | Observed diff is formatting/import ordering only. No separate new behavior requirements inferred. |
| `openspec/changes/add-windows-setup-and-watchdog/` | Existing proposal retained; add cold-start liveness coverage and implementation prerequisite. Installer/signing/hardware tasks remain open. |

## Follow-up gaps outside this implementation scope

- **Concurrency and readiness:** `with_retry` clones the driver without a whole-job lock; `check_connection` sends Init and the server schedules it every 30 seconds. The companion `improve-usb-printer-health` proposal defines job serialization/readiness interleaving and reproduction tasks; current per-write locking alone is not evidence of job isolation.
- **Duplicate delivery:** existing infinite retry plus consumer HTTP-to-Bluetooth fallback has no shared idempotency key. Do not claim exactly-once receipts or drawer pulses. A bounded retry/job-status redesign requires coordinated consumer decisions and its own proposal.
- **Sensor reporting:** existing specs already cover `X-Sensor-Key`, 60-second heartbeats, state/error events, and disabled reporting with an empty key. REIKA's `src/routes/api/sensors/report/+server.ts` and `src/lib/server/sensorDebounce.ts` retain ingestion but suppress escpos ntfy notifications. No new desktop notification feature is needed. Verify startup state and event recovery in a later reliability pass; the unstaged reporter diff is formatting only.
- **Trust configuration:** all-origin local CORS and disabled sensor TLS verification are explicit current behavior. Any change needs a compatibility/security proposal rather than being silently bundled into command parity.

## Verification boundary

This review validates proposal structure and evidence, not the uncommitted Rust implementation. No Rust behavior tests, USB printing, Windows installer tests, upstream deployment checks, or hardware capabilities are claimed. Local sibling commits are the inspected snapshot, not asserted to be deployed or latest on GitHub.
