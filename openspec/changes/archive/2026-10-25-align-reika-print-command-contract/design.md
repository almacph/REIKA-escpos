## Context

The browser validates with `REIKA/src/lib/model/schemas/receiptSchema.ts` and sends the same command envelope through HTTP or Bluetooth. The Rust enum currently lacks two command variants and uses a different MaxiCode tag. The separate `REIKA-printer-schemas` package contains old camelCase status/test schemas, whereas both live applications use snake_case. It is evidence of drift, not the authority for this change.

The unstaged work is predominantly formatting plus an end-to-end active italic implementation. The October regular-text requirement in REIKA explicitly supersedes active thermal italics. Preserve the useful compatibility parsing, but align output and preview with the newer consumer contract. This decision is part of the proposal, not an edit to the user's implementation.

## Goals / Non-Goals

Goals: accept consumer command payloads consistently, preserve saved receipts, produce deterministic error responses before printer side effects, and make command parity verifiable without hardware.

Non-goals: Bluetooth inside the desktop service, a new shared package, cash-drawer physical-state sensing, exactly-once printing, transport fallback changes, a job queue redesign, or a broad security/configuration migration.

## Decisions

1. **Compatibility at the boundary.** Keep the `{commands:[...]}` envelope and snake_case responses. Accept both MaxiCode spellings; keep the existing desktop serialization spelling to avoid rewriting saved logs solely for a tag change. `Italic` with a boolean is accepted and ignored on normal print, API reprint, GUI reprint, and preview paths. Do not inject italic-off bytes either.
2. **Pulse command parity.** `CashDrawerPulse` takes `{pin:"Pin2"|"Pin5",pulseMs:100|150|200}`. Match the consumer encoder's five bytes: `1B 70 m (pulseMs/2) FF`, where `m` is 0 or 1. Preserve legacy `CashDrawer` behavior. Success means command delivery, never physical drawer opening. Physical timing still needs verification on supported hardware.
3. **Raster parity with bounded input.** `BitImage` takes positive integer pixel width/height and base64 1-bit row-major data; each row occupies `ceil(width/8)` bytes. Proposed initial limits are width 1–576 pixels, height 1–4096 pixels, and 294,912 decoded bytes per image; the full JSON body is limited to 4 MiB. These are service safety limits, not a claim that every supported printer has 576 dots. Reject violations before output. Use normal-mode `GS v 0` bands of at most 24 rows, following the consumer encoder. The final band contains the remaining rows. No new printer-width setting is needed for this change.
4. **Preflight before output.** Parse and validate the entire command sequence, including decoding raster data and checked length arithmetic, before creating printer side effects. Check deterministic library input errors for all existing commands too. Printer errors during a print attempt return to the requester without replay because a partial USB failure can happen after bytes reached the printer. Discovery and transport recovery prepare the service for later requests; this does not promise exactly-once physical delivery.
5. **Shared semantics across entry points.** Normal prints and both reprint entry points use the same validation/encoding behavior. Treat each BitImage as one visible content command for marker placement. Render its pixels in preview with preserved aspect ratio, fitting the preview width; italic transitions do not affect text. Keep reprints out of print history as the existing spec requires.
6. **Contract fixtures rather than a runtime dependency.** Store representative JSON cases and expected command bytes locally, annotate the originating consumer commit, and verify both request entry points. Include old desktop tags and persisted logs. Refresh fixtures when either side changes. Do not import the stale schema package into Rust or rename the current HTTP response fields.

## Risks / Trade-offs

- Proposed raster/body limits may reject unusually large existing consumer payloads. Validate representative diagnostic images before release and coordinate any limit adjustment with REIKA; test exact boundary values.
- A valid USB transfer does not prove physical printing, drawer movement, or support for a particular 2D symbol. Record hardware verification separately from serialization tests.
- Base `escpos-commands` already contains uncommitted italic wording. At apply/archive time reconcile it to this approved delta without discarding unrelated edits. Do not archive active italics as accepted behavior by accident.
- The consumer and desktop both have older documentation describing only 40 commands or skipped Bluetooth features. Scope this work to the desktop contract and record upstream documentation follow-up separately.

## Migration Plan

Implement model/validation and byte fixtures first, then shared execution, reprint/preview behavior, and endpoint coverage. Verify the actual consumer's diagnostics with the new desktop build before rollout. Keep existing command tags accepted and old print logs readable. Reconcile base specs and command tables at archive time. Rolling back to the prior binary removes support for new commands and may make logs containing them unreadable; preserve print logs and coordinate rollback with consumer usage.

## Review Decisions

The proposed defaults are legacy italic no-op behavior and the raster limits above. If active italic output is still wanted, it requires a coordinated change to REIKA's current thermal requirement rather than silently adopting the unstaged behavior. Windows printer capability validation remains a release task.
