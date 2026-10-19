# Change: Add Windows setup installer for WinUSB and watchdog

## Why

The Windows release currently requires an operator to download Zadig and manually replace the printer's USB driver before the REIKA printer service can communicate with it. The release also packages a VBS watchdog without registering it or checking whether the service is responsive, so a hung process can remain running but unusable.

## What Changes

- Add a Windows setup package that installs the REIKA printer service and a signed WinUSB driver package for the supported printer identifiers.
- Replace the manual Zadig setup step for supported devices with an elevated installer flow that detects or installs the required driver.
- Register the watchdog through Windows Task Scheduler during setup and configure it to start with the user's Windows session.
- Add a lightweight service liveness endpoint that the watchdog can probe without sending a print job.
- Upgrade watchdog behavior to distinguish a missing or hung service from a printer that is temporarily unplugged, using bounded HTTP probes, consecutive-failure thresholds, and restart cooldowns.
- Document administrator requirements, driver replacement behavior, supported USB identifiers, installation, repair, and removal.

## Impact

- Affected specs: new `windows-deployment` capability; HTTP API liveness support.
- Affected code: Windows packaging, driver assets, watchdog setup, service health routing, and release documentation.
- Release requirement: the WinUSB driver package must be signed and include the INF, catalog, and required installation metadata before production distribution.
- The existing Zadig workflow remains available as a development and recovery fallback, but is no longer the normal end-user setup path.

## Review expansion: integrated driver setup

The setup flow SHALL prefer Windows' in-box WinUSB driver, with a device-specific signed INF/CAT package, rather than requiring users to install libusb0 manually. Add device/driver diagnostics, a GUI Setup/Repair entry point, a narrow elevated helper, exact interface selection, post-install verification, and repair outcomes for existing USBPRINT/libusb0/libusbK bindings. A successful existing WinUSB binding requires no reinstall. See the expanded design and scenarios for driver ranking, multiple devices, absent printers, elevation cancellation, and production signing gates.

Cold-start liveness is a prerequisite: currently `src/main.rs` starts HTTP only after `try_open` succeeds. Start `/health` independently of initial USB discovery to avoid watchdog restart loops when no printer is attached.

Runtime state storage and standard-user execution depend on the storage contract in `modernize-desktop-platform`; installed deployments must not write mutable state under Program Files.
