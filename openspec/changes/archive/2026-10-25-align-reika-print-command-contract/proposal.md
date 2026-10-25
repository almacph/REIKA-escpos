# Change: Align the desktop printer contract with REIKA

## Why

REIKA currently produces command payloads that this service rejects (`CashDrawerPulse`, `BitImage`, and `Maxicode`). The unstaged desktop changes add active italic printing and preview styling, while REIKA's current thermal contract requires legacy `Italic` commands to remain accepted but have no visual or printer effect. Shipping the unstaged behavior unchanged would create different results across HTTP and Bluetooth printing.

## What Changes

- Retain the unstaged `Italic(bool)` compatibility parser and round-trip coverage, but replace active italic output, preview state, and reprint restoration with no-op behavior.
- Add explicit pulse-duration drawer commands and bounded raster-image commands using REIKA's existing JSON shapes.
- Accept REIKA's `Maxicode` spelling while retaining the existing `MaxiCode` spelling for saved desktop payloads.
- Validate complete jobs before any USB output; reject invalid new command parameters with the existing JSON error shape rather than retrying deterministic invalid input indefinitely.
- Include raster content in reprint midpoint calculation and desktop preview; preserve existing marker and print-history behavior. Printer errors during a print attempt return without replay; transport recovery continues for later requests.
- Establish fixture-based contract verification against the actual REIKA consumer and document the older schema package's incompatible field names.
- Repair missing Purpose sections and stale command/reference documentation during implementation and archive, preserving all existing requirements.

**Compatibility change:** payloads containing `Italic` remain accepted, but the active italic behavior in the uncommitted working tree is deliberately superseded by REIKA's regular-text contract. No new endpoint, response rename, or automatic migration to the older schema package is proposed.

## Impact

- Affected specs: `escpos-commands`, `http-api`, `printer-service`, `gui`; structural documentation repair also touches `usb-driver` and `openspec/project.md`.
- Affected code after approval: command models, print/reprint handlers and execution, receipt renderer, reprint content classification, contract fixtures and documentation.
- Consumer evidence: REIKA at `0e5fcc1f`; desktop baseline `a687806` plus the existing unstaged changes; schema package at `2b32cec`. See [evidence.md](evidence.md).
- The existing `add-windows-setup-and-watchdog` proposal owns installer, liveness, and watchdog changes. Its cold-start coverage is strengthened separately; this proposal does not duplicate that capability.
- Proposal only: implementation and hardware verification remain unchecked. Existing unstaged source and base-spec edits are preserved for review.
