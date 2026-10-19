# Desktop stack review — 2026-10-18

## Recommendation

Keep Rust, Tokio, rusb, and egui/eframe. This small local printer service does not need a browser shell, cloud backend, or a framework rewrite. Upgrade the existing stack in testable groups and prioritize reliability and installability over cosmetic modernization.

The table compares the actual Cargo.lock with stable releases returned by the crates.io API during this review. Manifest versions are compatibility constraints, not necessarily the versions installed. A newer release is an upgrade candidate, not proof of a vulnerability or a tested compatible replacement.

| Dependency | Manifest | Locked versions | Latest stable observed | Latest minimum Rust |
|---|---|---|---|---|
| [log](https://crates.io/crates/log) | 0.4 | 0.4.29 | 0.4.34 | 1.71.0 |
| [env_logger](https://crates.io/crates/env_logger) | 0.11.3 | 0.11.3 | 0.11.11 | 1.71 |
| [escpos](https://crates.io/crates/escpos) | 0.17.0 | 0.17.0 | 0.20.1 | 1.88.0 |
| [rusb](https://crates.io/crates/rusb) | 0.9.4 | 0.9.4 | 0.9.4 | not declared |
| [serde](https://crates.io/crates/serde) | 1.0.203 | 1.0.228 | 1.0.229 | 1.56 |
| [serde_json](https://crates.io/crates/serde_json) | 1.0.118 | 1.0.120 | 1.0.154 | 1.71 |
| [tokio](https://crates.io/crates/tokio) | 1.38.0 | 1.38.0 | 1.53.3 | 1.71 |
| [warp](https://crates.io/crates/warp) | 0.3.7 | 0.3.7 | 0.4.3 | not declared |
| [eframe](https://crates.io/crates/eframe) | 0.29 | 0.29.1 | 0.37.0 | 1.95 |
| [egui](https://crates.io/crates/egui) | 0.29 | 0.29.1 | 0.37.0 | 1.95 |
| [tray-icon](https://crates.io/crates/tray-icon) | 0.19 | 0.19.3 | 0.26.1 | 1.90 |
| [toml](https://crates.io/crates/toml) | 0.8 | 0.8.2 | 1.1.8+spec-1.1.0 | 1.85 |
| [chrono](https://crates.io/crates/chrono) | 0.4 | 0.4.43 | 0.4.45 | 1.62.0 |
| [notify-rust](https://crates.io/crates/notify-rust) | 4 | 4.11.7 | 4.18.2 | 1.89.0 |
| [reqwest](https://crates.io/crates/reqwest) | 0.12 | 0.12.9 | 0.13.5 | 1.85.0 |
| [windows](https://crates.io/crates/windows) | 0.58 | 0.52.0, 0.58.0, 0.61.3 | 0.62.2 | 1.82 |
| [winit](https://crates.io/crates/winit) | 0.30 | 0.30.12 | 0.30.13 | 1.70.0 |

The direct windows dependency is 0.58.0; the other versions listed are transitive packages with the same name. Local rustc is 1.92.0; CI floats on stable. Latest egui/eframe requires 1.95, so a toolchain change precedes that target. Cargo.lock is committed but the build script omits `--locked`.

## Upgrade order

1. **Establish release checks and audit first.** Run a lockfile advisory scan, build/test on native Windows, record supported OS/architecture, and pin a tested toolchain. No cargo-audit executable is installed in this environment, so this review does not assert that the graph is vulnerability-free. A full transitive advisory/license review remains a task.
2. **Compatible updates:** Tokio 1.x, serde/serde_json, log, chrono, notify-rust, winit, and compatible reqwest 0.12 patch updates. Re-resolve in an isolated branch and check MSRV/features. Remove env_logger and the direct winit dependency only if a dependency-usage check confirms they are unnecessary; no runtime imports were found in src (winit appears only as a log-filter string).
3. **Printing:** escpos 0.17 → 0.20.1, isolated from GUI and USB lifecycle changes. Validate command fixtures, buffering/final flush, cut behavior, and custom driver trait compatibility on both printers. rusb 0.9.4 is already the latest observed stable; inspect libusb1-sys/native linkage and transitive advisories rather than forcing a rusb replacement.
4. **HTTP/config:** Warp 0.3 → 0.4, reqwest 0.12 → 0.13, TOML 0.8 → 1.x as separate migrations. Verify feature flags, response/rejection types, TLS certificate configuration, sensor timeout behavior, and old config files. Staying on Warp is a smaller change than migrating to Axum; no product need for a server framework switch was found. [Warp releases](https://github.com/seanmonstar/warp/releases).
5. **GUI/Windows:** upgrade egui and eframe together, then tray-icon and direct Windows bindings against the resolved windowing stack. The eframe changelog documents replacing App::update with logic/ui in 0.34; the current app implements update, so this is an API migration, not a version-only bump. Validate tray clicks, foreground restore, close-to-tray, singleton behavior, DPI, and shutdown. [eframe changelog](https://github.com/emilk/egui/blob/main/crates/eframe/CHANGELOG.md), [release/MSRV policy](https://github.com/emilk/egui/blob/main/RELEASES.md).

## Product and operations additions

| Priority | Addition | Reason / scope |
|---|---|---|
| P0 | Non-destructive USB health and serialized recovery | Current Init probes and per-write locking can interfere with printing; the dedicated printer-health proposal owns this. |
| P0 | Integrated driver setup and independent /health | Existing setup proposal now covers WinUSB diagnosis, selected-device repair, signing, and absent-printer startup. |
| P1 | Native Windows CI and reproducible releases | Current workflow only cross-builds and releases every main push; it does not run tests/audit or Windows tray/driver smoke checks. |
| P1 | Writable, durable state | Config/history/logs currently live beside the executable, incompatible with a normal protected install location. Use per-user state and atomic writes with recoverable migration. |
| P1 | Logs surviving restarts | File logger truncates on startup; a watchdog restart erases the evidence. Add bounded rotation and redacted diagnostic export. |
| P1 follow-up | Local API trust and sensor TLS | All-origin CORS and danger_accept_invalid_certs(true) exist today. Design allowed origins plus explicit local enrollment/token handling and a trusted private CA path with REIKA; CORS alone is not authentication. Do not break existing self-signed deployments by flipping a flag without migration. |
| P2 follow-up | Job IDs, bounded queue, delivery uncertainty | Coordinate idempotency/status/cancellation with REIKA before replacing existing infinite retry. HTTP-to-Bluetooth fallback must not silently duplicate already delivered receipts or drawer pulses. |
| P2 follow-up | Signed update discovery and rollback | Start with visible version/check-for-update and a verified signed installer; defer unattended self-update until signing, data migration, maintenance coordination, and rollback are proven. |
| P2 follow-up | Printer profiles and selection | Persist selected serial/port identity when needed, paper width, and verified status capability; VID/PID alone is ambiguous with identical printers. Health proposal prevents silent switching meanwhile. |

Do not add a database merely for the existing bounded receipt history. A durable job queue would be a separate requirement with persistence and replay semantics. Do not claim drawer-open detection or completed paper output from successful USB writes.

## Boundaries

No dependencies or toolchains were changed. Crates.io registry results are recorded in dependency-snapshot.json. Version freshness was checked; compilation, vulnerability scanning, Windows operation, licensing compatibility, and physical hardware validation remain implementation checks.
