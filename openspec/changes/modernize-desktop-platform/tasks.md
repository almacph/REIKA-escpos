## 1. Release baseline
- [x] 1.1 Run a full lockfile advisory/license/duplicate dependency review and record target reachability; confirm unused env_logger/direct winit before removing either.
- [x] 1.2 Pin a tested toolchain and add format, build, meaningful unit/contract tests, native Windows tests, and locked cross-build gates before release.
- [x] 1.3 Add dependency-update automation and artifact provenance/version/hash reporting; prevent publication when required checks fail.

## 2. State and diagnostics (installer prerequisite)
- [x] 2.1 Implement explicit portable/installed storage resolution and safe original-user migration with conflict handling.
- [x] 2.2 Add atomic configuration/history writes, last-known-good recovery, and visible persistence errors without reprinting.
- [x] 2.3 Add bounded append/rotation and redacted diagnostics; verify restart evidence survives and retention limits hold.
- [ ] 2.4 Test read-only install directories, interrupted writes, corrupt files, migration conflicts, upgrade/rollback, and standard-user execution.

## 3. Dependency groups
- [x] 3.1 Refresh compatible versions in an isolated change and validate existing API/config/printing contracts.
- [x] 3.2 Migrate escpos separately and verify command bytes, buffering/cutting, driver integration, and both hardware presets.
- [x] 3.3 Migrate Warp, reqwest, and TOML in separate reviewed steps with rejection, TLS, heartbeat, and old-config tests.
- [x] 3.4 Select compatible toolchain/egui/eframe versions and migrate GUI APIs; update tray/Windows dependencies with native smoke checks.
- [x] 3.5 Re-run advisory checks and publish a resolved version/MSRV report and rollback notes for each group.

## 4. Completion
- [ ] 4.1 Validate strict OpenSpec, record Windows/hardware results, and archive only after the approved stages are deployed.

## Local verification status

- Local portions of 2.4 and 4.1 passed: 24 native tests, Windows GNU compile, format, OpenSpec, zero-vulnerability full-lock audit, and target-scoped cargo-deny. Native Windows ACL/installer, hardware, deployment and archive portions remain open. Evidence: `.agent/artifacts/2026-10-19-0857-modernize-storage-audit-final-acceptance.md` and `.agent/artifacts/2026-10-19-0850-modernize-dependency-audit-final.md`.
