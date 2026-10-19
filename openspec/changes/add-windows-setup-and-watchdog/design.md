## Context

The service uses `rusb` to claim the printer's USB interface directly. Windows therefore needs the printer bound to WinUSB instead of the standard Windows USB printer driver. The current release contains `reika-escpos.exe` and `reika-watchdog.vbs` in a zip, while driver installation and watchdog registration are manual.

The service currently exposes `GET /print/test`, but that endpoint performs a printer connection check. A watchdog needs a process/API liveness probe that remains meaningful while the printer is disconnected, otherwise a normal printer outage can be confused with an application failure.

## Goals

- Make supported printer setup a one-time elevated installer flow.
- Remove the normal requirement for users to operate Zadig.
- Start the service and watchdog reliably at user logon.
- Restart the service when it is absent or unresponsive, while allowing its existing USB reconnection loop to handle a disconnected printer.
- Make installation, repair, and removal reviewable and reversible.

## Non-Goals

- Automatically replace arbitrary USB drivers for devices outside the supported printer identifiers.
- Preserve standard Windows printer-spooler access to a printer that is bound to WinUSB.
- Install a driver silently from the running service without an explicit elevated setup action.
- Replace the existing custom USB driver or change the ESC/POS command protocol.

## Decisions

### Driver installation

The release should contain a device-specific WinUSB driver package covering the supported presets (`0483:5840` and `0FE6:811E`). The elevated helper should stage the reviewed package with Windows PnP driver-store tooling, bind only the selected device/interface through the targeted installation path described below, and verify the resulting binding before reporting printer readiness.

The package must include a signed catalog. The installer must fail with a clear repair instruction if the package is missing, invalid, or rejected by Windows. A production release must not rely on a self-signed catalog or test-signing mode.

### Installer boundary

Driver installation belongs in a setup/repair package rather than the normal service startup path. This keeps the runtime service least-privileged and prevents an unexpected driver replacement when the service starts.

The installer should install the executable and supporting files, create or update the watchdog task, and offer repair/uninstall behavior. The exact installer technology can be selected during implementation as long as it supports elevation, signed driver package deployment, task registration, and clean removal.

### Health and watchdog behavior

Add a lightweight `GET /health` endpoint that proves the HTTP server is responsive and returns service metadata. It must not require a successful USB probe and must not print or move paper. Keep `GET /print/test` as the printer-readiness check.

The watchdog should check, in order:

1. The expected executable exists and only one instance is running.
2. The configured localhost port accepts a connection.
3. `GET /health` returns a valid success response within a bounded timeout.

It should restart only after a small configurable number of consecutive failures, apply a restart cooldown, and log the reason. A failed printer readiness check alone must not restart a healthy service process; the service's internal USB retry loop owns that recovery.

### Startup model

The service is a GUI/tray application, so the watchdog task should run in the interactive user session rather than as a detached system service. The installer should register the task at user logon and ensure duplicate task registration is idempotent.

## Open Release Decision

The implementation must confirm how the WinUSB package will be signed and distributed. If a signed package is not available yet, the first implementation may provide a setup flow that detects the missing binding and launches a documented Zadig fallback, but that fallback does not satisfy the final plug-and-play release requirement.


## Integrated setup implementation design (2026-10-18 review)

### Driver choice

`rusb` calls userspace libusb; that does not install a Windows device driver. WinUSB is the default backend for supported printers. libusb0 is the older libusb-win32 driver, not a second package every user needs. Detect existing libusb0/libusbK installations and offer explicit migration to WinUSB; do not install all backends. Existing alternative bindings that actually open can be reported as legacy-compatible, with migration deferred until the user invokes setup. They are not the supported fresh-install target. The libusb project recommends WinUSB and notes that libusb0/libusbK access needs libusbK.dll. [libusb Windows guidance](https://github.com/libusb/libusb/wiki/Windows).

Windows can automatically bind WinUSB when device firmware advertises the appropriate descriptors. Do not assume third-party thermal printers do this or propose changing their firmware. For these presets, ship a matching INF with an interface GUID and signed catalog referencing the system WinUSB driver. App signing and driver-package signing are distinct release requirements. [Microsoft WinUSB installation](https://learn.microsoft.com/en-us/windows-hardware/drivers/usbcon/winusb-installation).

### Proposed repository components

- `src/app/driver_setup.rs`: read-only device inventory and GUI setup/repair state; states include absent, ready, needs-driver, legacy-compatible, ambiguous-selection, access-denied, repair-failed, and reboot-required.
- `src/bin/reika-driver-setup.rs`: Windows-only elevated helper using SetupAPI/NewDev through windows-rs. Accept only an enumerated supported device instance/interface and the packaged driver manifest. Validate identity again after elevation. Do not accept arbitrary commands, INF paths, or VID/PID values from the HTTP API.
- `installer/driver/`: reviewed supported-ID/interface manifest, INF source, signing/package instructions, and verified release catalog input. Capture actual composite interface IDs from each printer before finalizing matches; do not assume interface zero for every model.
- `installer/`: setup/repair packaging, helper launch, watchdog registration, and uninstall policy. Use an installer EXE with a standard-user interactive app launch after elevated setup; implementation selects the installer technology after checking its redistribution/signing requirements.
- Windows release CI: build helper and application, verify signed package provenance and executable signatures, package artifacts, and retain hashes/version metadata. Do not fabricate or self-sign a production catalog locally.

These are proposed files, not files implemented by this review.

### Device selection and installation transaction

1. Enumerate hardware IDs, device instance ID, composite interface IDs, current driver service/provider/version, and connection/problem status without elevation. List matching devices if more than one exists. Manual/unknown VID/PID devices are diagnostic-only until a reviewed driver package supports them.
2. Show the selected printer/interface and explain that replacing USBPRINT changes access through the Windows spooler. Invoke the narrow helper through UAC only for the explicit install/repair operation. UAC cancellation returns to the GUI without changing the device.
3. Pause the installation's watchdog and stop/release the service's USB handle before rebinding. Preserve the prior driver identity and an operation log. The helper revalidates the selected instance/interface and package signature before making changes.
4. Stage only the bundled signed package with PnPUtil. For a present device use a selected-device SetupAPI/NewDev binding path; do not use a blanket `pnputil /add-driver ... /install` as a selected-device primitive because it updates matching devices, and does not force a lower-ranked driver. Enumerate/select the intended driver and call the supported device-install API for the selected instance. Use force selection only within the explicitly reviewed repair transaction and report the actual outcome. [PnPUtil behavior](https://learn.microsoft.com/en-us/windows-hardware/drivers/devtest/pnputil-command-syntax), [DiInstallDevice](https://learn.microsoft.com/en-us/windows/win32/api/newdev/nf-newdev-diinstalldevice).
5. Re-enumerate the same target; verify WinUSB binding and expected interface access with a nonprinting probe after helper handles are released. If Windows still selects another driver, report binding failure rather than success. Handle reboot/replug-required separately. A successful package-store operation alone is not proof of a working printer.
6. Resume watchdog/application in the original interactive standard-user session after completion or failure, using printer-independent liveness. Keep partial outcomes and repair details visible. If the old binding can be safely restored after a failed transaction, offer repair/restore; do not silently remove system/shared drivers.

For an absent printer, stage the package and report “waiting for printer,” not “printer ready.” Future device instances may match the staged package, but need binding verification when connected. Replacing a printer with a different model/VID/PID requires a new supported manifest entry and validation; installation cannot make arbitrary hardware supported.

### Release boundaries and licensing

First target the repository's existing x64 Windows build. Do not claim ARM64 or other targets without builds and driver validation. Validate clean Windows 10/11 environments according to the supported deployment matrix, including driver signing policy and Secure Boot. Production distribution requires the signed catalog and a trusted installer/helper release. The signing identity, certificate/account availability, actual composite IDs, and representative devices are unresolved external release inputs.

A libwdi-based embedded Zadig-style flow is an alternative, not the default in this proposal. It introduces additional licensing/distribution and signing-policy questions; the upstream project carries LGPL licensing. Review those obligations and deployment behavior before adopting it. A guided Zadig fallback is acceptable for development/recovery, but must not be labeled completion of signed integrated setup. [libwdi license](https://github.com/pbatard/libwdi/blob/master/COPYING).

### Cold-start and watchdog coordination

Create HTTP service state before opening USB; readiness must represent absent/uninitialized hardware. `/health` must respond throughout initial discovery, reconnection, and setup repair. Keep USB retries off the HTTP liveness path. A driver repair is an explicit maintenance state for the watchdog, with bounded recovery if the helper crashes. Existing retry semantics for print jobs must be preserved or changed through a separately approved runtime proposal.

### Storage and uninstall

Installed binaries/helper/package live in a protected install location. Configuration, print history, and logs use the original interactive user's writable data directory; the elevated helper must not accidentally migrate them into the administrator's profile. The companion platform proposal defines the migration. Uninstall removes tasks and binaries, asks whether to retain user data, and leaves shared driver-store packages unless safe removal is established. Document restoration to the vendor/USBPRINT driver without promising automatic restoration in every case.
