## 1. Driver package and installer foundation

- [ ] 1.1 Confirm the supported hardware IDs, interface matching rules, and whether both Standard and ICS Advent presets are included.
- [ ] 1.2 Produce a WinUSB INF/CAT package for the supported identifiers and document the production signing path.
- [x] 1.3 Select and configure an installer format that supports elevation, driver package installation, executable packaging, scheduled task registration, repair, and uninstall.
- [x] 1.4 Add installer validation for package signature, supported device binding, and actionable failure messages.

## 2. Service liveness

- [x] 2.1 Add `GET /health` with a bounded, printer-independent success response containing service version or uptime metadata.
- [x] 2.2 Add focused route/handler tests for healthy responses and confirm the endpoint does not issue USB or print operations.
- [x] 2.3 Keep `/print/test` as the printer readiness endpoint and document the distinction between service liveness and printer readiness.

## 3. Watchdog supervision

- [x] 3.1 Replace process-only polling with process, localhost-port, and `/health` checks using bounded timeouts.
- [x] 3.2 Add consecutive-failure thresholds, restart cooldown/backoff, duplicate-instance protection, and structured watchdog logging.
- [x] 3.3 Register the watchdog through the installer as an idempotent user-logon scheduled task.
- [ ] 3.4 Verify that a printer unplugged while the service remains responsive does not trigger a service restart.
- [ ] 3.5 Verify that a crashed or hung service is restarted and that the mutex prevents duplicate service instances.

## 4. Documentation and release validation

- [x] 4.1 Update README setup instructions from manual Zadig steps to the installer flow, including administrator and driver-replacement behavior.
- [x] 4.2 Document repair/uninstall and the Zadig fallback for development or recovery cases.
- [ ] 4.3 Update the Windows build/package script to produce the installer artifact and retain required release files.
- [ ] 4.4 Test a clean Windows machine with each supported printer preset, including first install, reboot/logon, printer unplug/replug, repair, uninstall, and watchdog restart scenarios.
- [x] 4.5 Run the repository's static checks and targeted Rust tests; record Windows-only validation separately where cross-compilation cannot exercise driver installation.


## 5. Integrated setup and repair (review additions)

- [ ] 5.1 Before implementation of section 1, capture real device/interface IDs and current binding data for both presets; define the allowlisted package manifest and signing ownership.
- [x] 5.2 Add read-only driver diagnostics and a GUI Setup/Repair entry point that distinguishes absent, wrong-driver, legacy-compatible, permission, ambiguous-selection, and ready states.
- [x] 5.3 Build a narrow elevated helper with allowlisted instance/interface validation and verified package selection; handle UAC cancellation without mutation.
- [ ] 5.4 Stage the signed package and implement selected-device binding/verification; test lower-ranked driver outcomes and ensure unrelated devices/interfaces are untouched.
- [x] 5.5 Coordinate handle release, watchdog maintenance mode, helper crash recovery, replug/reboot outcomes, and resumption as the original standard user.
- [x] 5.6 Before section 2 health validation, decouple HTTP startup from initial USB discovery and test cold start with no printer attached.
- [ ] 5.7 Integrate the companion platform proposal's writable state migration and preserve original-user identity across elevation; test Program Files installation as a standard user.
- [ ] 5.8 Extend the Windows matrix to USBPRINT, WinUSB, libusb0/libusbK, multiple identical printers, composite interfaces, unknown devices, moved USB ports, declined elevation, invalid catalog, repair failure, and absent-printer setup.
- [ ] 5.9 Verify production signing inputs and redistribution obligations; retain an explicitly labeled recovery fallback when signed assets are unavailable and do not mark plug-and-play delivery complete.


## Local implementation evidence and release gates

Local driver helper, setup/repair UI, maintenance coordination, installer and watchdog code were implemented and audited. See `.agent/artifacts/2026-10-19-1848-packaging-final-acceptance.md` and `.agent/artifacts/2026-10-19-1847-windows-setup-lease-lifecycle.md`.

Unchecked tasks retain genuine external portions: real device/interface capture (1.1/5.1), signed production INF/CAT and redistribution/signing ownership (1.2/5.9), live selected-device/lower-ranked binding verification (5.4), standard-user Program Files/elevation validation (5.7), Windows crash/unplug/logon/repair matrices (3.4/3.5/4.4/5.8), and production installer compilation/signing (4.3). The local development ZIP and final static/test results are recorded separately when completed. No production plug-and-play delivery or deployment is claimed.

Final local gates passed: 53 application and 11 helper tests, strict Clippy, formatting, Windows GNU compile and test compilation, cargo audit (zero vulnerabilities), cargo deny, and strict OpenSpec (10/10). Final Windows release build produced the development/recovery ZIP and provenance; signed production installer remains gated. Evidence: `.agent/artifacts/2026-10-19-1857-final-verification-build.md`; final package audit: `.agent/artifacts/2026-10-19-1854-windows-setup-watchdog-final-acceptance.md`.
