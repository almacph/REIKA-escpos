## Context

See stack-review.md for actual locked versions and evidence. Existing Rust architecture is appropriate. Upgrade boundaries must make driver/runtime regressions distinguishable from GUI API migration issues.

## Decisions

- Pin a tested Rust toolchain compatible with the chosen dependency group. The latest observed eframe/egui target needs Rust 1.95; local 1.92 cannot build that target. Keep both GUI crates aligned and verify windowing/tray compatibility before changing APIs.
- Keep native Windows tests and GNU cross-build coverage. Do not switch the release target implicitly. Use Cargo.lock and --locked, gate release on tests/audit, and record hashes, commit, toolchain, and versions. Advisory exceptions require documented rationale and expiry; “old” and “vulnerable” are different findings.
- Installed mode uses `%LOCALAPPDATA%/REIKA-escpos` for config, bounded receipt history, and logs. Portable mode explicitly retains executable-directory storage. Default existing portable ZIP deployments to portable mode during migration; the installer opts into installed mode. Both modes use the same resolver and display the resolved state directory in settings/diagnostics.
- Migrate legacy files only when the destination is absent; validate before atomic replacement, preserve originals, and report conflicts. Use original-user identity across elevated setup. Never silently replace corrupt config/history with saved defaults; preserve the corrupt file and expose recovery.
- Atomic writes use a same-directory temporary file, flush, and Windows-compatible replacement; retain a last-known-good copy. Failed persistence must be visible but must not re-send an already delivered receipt.
- Logging remains opt-in. When enabled, append/rotate rather than truncate; proposed limits are 5 MiB per file and three retained rotated files plus the active file. Diagnostic export excludes credentials and receipt command/text content by default. Keep minimal version, backend, USB error category, state transitions, attempt IDs, and timestamps.

## Sequencing

Release gates first; storage/logging before installed distribution; compatible dependency updates; printing changes; HTTP/config migrations; GUI/toolchain migration last. Keep each dependency group independently reviewable and revertible. Coordinate command parity and health fixtures with their proposals; they do not require a wholesale dependency refresh first.

## Risks / Open Inputs

Native Windows and physical printers remain required for GUI/driver verification. Audit tooling is absent locally; install/run it during implementation instead of claiming a clean report. Registry latest versions are a dated candidate list; recheck releases and advisories before applying. Final toolchain and GUI target must be selected together, and installer signing remains owned by the deployment proposal.
