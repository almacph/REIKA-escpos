# Change: Make USB printer health non-destructive and recoverable

## Why

Current periodic and HTTP checks send Init, map write success to a boolean, and do not recover a failed idle connection. USB access is locked per write rather than per receipt, blocking native calls run inside async tasks, reconnection opens a replacement before releasing the retained handle, and manual configuration guesses the IN endpoint. These paths can produce misleading status, contention, and recovery that depends on a new print request. These are code-review findings, not a reproduced hardware incident.

## What Changes

- Separate process liveness, USB transport availability, and verified physical printer readiness with timestamps and reasons.
- Replace Init-based health checks with cached status plus serialized, non-destructive capability-aware probes.
- Use one blocking USB worker to own the device, whole print attempts, status transactions, and reconnect lifecycle.
- Recover while idle, release stale handles before reopen, validate actual descriptors, and support output-only devices without invented input endpoints.
- Bound health work, coalesce refreshes, classify transient versus hard failures, and prevent stale observations from reversing newer state.
- Preserve GET /print/test and sensor vocabulary with documented compatibility mapping; add a detailed read-only GET /printer/status endpoint and richer desktop status.

## Impact

Affected specs: printer-service, usb-driver, http-api, gui. Affected code after approval: src/main.rs, server, services, status models/handlers, GUI/tray, and sensor reporting. Depends on the cold-start /health work in add-windows-setup-and-watchdog. Coordinate shared execution with align-reika-print-command-contract; no dependency major upgrade is required first.

This proposal supersedes the existing mandatory Init health probe and fixed initialization retry delay. Discovery retries remain indefinite, while a printer error during an active print request is returned without replaying that job because output may already have reached the printer. Durable queues, cancellation, automatic transport fallback, and exactly-once physical delivery remain separate work. Implementation and physical printer verification are not performed in this review.
