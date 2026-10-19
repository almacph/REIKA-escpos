# Change: Modernize desktop dependencies and operational foundations

## Why

The service has an aging lockfile, no test/audit gate before releases, and mutable files beside the executable. The latter prevents dependable standard-user operation under a protected installation directory. Debug logs are truncated at every startup, hiding the cause of watchdog restarts.

## What Changes

- Upgrade dependencies in separately validated groups, beginning with compatible patches and a pinned toolchain; keep the current architecture.
- Add Windows validation, locked builds, dependency/security checks, and release metadata before publishing.
- Add installed-mode per-user state, non-destructive migration, atomic configuration/history writes, and bounded diagnostic retention.
- Document broader follow-ups in [stack-review.md](stack-review.md), including API trust, TLS enrollment, job idempotency, and verified updates. Those follow-ups are recommendations, not silently included implementations.

## Impact

Affected specs: new desktop-platform and modified configuration. Affected files after approval: Cargo.toml/lock, toolchain, workflows/build script, app configuration/history/logger, and their GUI error reporting. The Windows installer proposal depends on the storage migration; GUI upgrades can ship later independently. See dependency-snapshot.json for observed registry versions. No code or version changes are made by this proposal.
