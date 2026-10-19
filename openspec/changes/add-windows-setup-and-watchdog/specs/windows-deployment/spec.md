## ADDED Requirements

### Requirement: Supported WinUSB Driver Installation

The Windows setup package SHALL install and verify a signed WinUSB driver package for every supported printer preset before declaring a connected supported printer ready.

#### Scenario: Standard printer setup

- **WHEN** setup runs with a supported Standard printer connected
- **THEN** it installs or verifies WinUSB for `VID_0483 PID_5840`
- **AND** the service can claim the configured USB interface after setup completes

#### Scenario: ICS Advent printer setup

- **WHEN** setup runs with a supported ICS Advent printer connected
- **THEN** it installs or verifies WinUSB for `VID_0FE6 PID_811E`
- **AND** the service can claim the configured USB interface after setup completes

#### Scenario: Driver package rejected

- **WHEN** Windows rejects the driver package signature or package metadata
- **THEN** setup stops before starting the service
- **AND** shows an actionable error identifying driver installation as the failed step

#### Scenario: Printer is not connected during setup

- **WHEN** setup runs without a connected supported printer
- **THEN** it installs the signed package and service files if possible
- **AND** reports that printer binding will be completed when the supported device is connected

### Requirement: Elevated and Reversible Setup

The setup package SHALL request administrator approval for driver and scheduled-task changes and SHALL provide repair and uninstall behavior.

#### Scenario: First installation

- **WHEN** a user starts setup
- **THEN** Windows displays the required elevation prompt
- **AND** setup installs the service, driver package, and watchdog task as one guided flow

#### Scenario: Repair installation

- **WHEN** the user runs setup on an existing installation
- **THEN** setup can reapply the driver package and recreate the watchdog task without creating duplicate tasks or service instances

#### Scenario: Uninstall

- **WHEN** the user removes the installation
- **THEN** the executable, watchdog task, and application files are removed according to the installer's policy
- **AND** the user is told how to restore the original Windows printer driver if the device was previously used for standard printing

### Requirement: Service Liveness Endpoint

The service SHALL expose `GET /health` as a lightweight liveness endpoint that does not require printer readiness and does not send data to the printer.

#### Scenario: Service is responsive

- **WHEN** `GET /health` is requested while the service HTTP server is responsive
- **THEN** it returns a successful JSON response containing service health metadata

#### Scenario: Printer is disconnected

- **WHEN** the service is running but the printer is unplugged
- **THEN** `GET /health` remains available
- **AND** printer readiness remains reported separately through `GET /print/test`

### Requirement: Registered Watchdog Supervision

The Windows setup package SHALL register an idempotent user-logon watchdog task that can restart the printer service when the process is absent or unresponsive.

#### Scenario: Service process is absent

- **WHEN** the watchdog observes that the expected service process is not running
- **THEN** it starts the packaged service executable

#### Scenario: Service process is hung

- **WHEN** the process exists but the localhost port or `GET /health` fails repeatedly within the configured timeout
- **THEN** the watchdog restarts the service after the configured consecutive-failure threshold
- **AND** applies a cooldown before attempting another restart

#### Scenario: Printer is temporarily unavailable

- **WHEN** the service process and `GET /health` are responsive but `GET /print/test` reports the printer offline
- **THEN** the watchdog does not restart the service
- **AND** the service's USB reconnection loop remains responsible for printer recovery

#### Scenario: Duplicate watchdog registration

- **WHEN** setup or repair is run more than once
- **THEN** only one watchdog task remains registered for the installation
- **AND** the service's existing single-instance protection continues to prevent duplicate processes


### Requirement: Printer Driver Diagnosis and Targeted Repair

The application SHALL expose a read-only driver diagnosis and an explicit Setup/Repair action for supported devices. It SHALL prefer WinUSB for fresh installations and distinguish absent hardware, valid WinUSB, legacy libusb0/libusbK, unsupported driver binding, access failures, and ambiguous multiple-device selection. Repair SHALL target only an explicitly selected supported device instance/interface and SHALL NOT rebind unrelated USB devices or composite interfaces.

#### Scenario: Printer uses the standard Windows printing driver
- **WHEN** a supported printer is bound to USBPRINT
- **THEN** the GUI explains the required WinUSB binding and offers elevated setup
- **AND** the user is informed of the effect on normal Windows spooler access before replacement

#### Scenario: WinUSB already works
- **WHEN** the selected printer has a verified WinUSB binding and interface access
- **THEN** setup reports readiness without reinstalling its driver

#### Scenario: Legacy libusb driver
- **WHEN** a supported device is bound to libusb0 or libusbK
- **THEN** diagnostics identifies the existing backend and tests access
- **AND** migration to WinUSB occurs only through the explicit repair operation

#### Scenario: Multiple or unknown printers
- **WHEN** multiple devices match or the device is outside the supported manifest
- **THEN** setup requires unambiguous supported-device selection or reports the device unsupported
- **AND** it does not replace unrelated drivers

### Requirement: Verified Elevated Driver Transaction

Driver installation SHALL run in a narrow elevated helper that revalidates selected hardware identity and the packaged signed driver before mutation. It SHALL release printer handles, coordinate watchdog maintenance, verify actual device binding after installation, and report cancellation, failed binding, waiting-for-device, and reboot-required outcomes distinctly. A staged package SHALL NOT by itself count as printer readiness.

#### Scenario: Elevation declined
- **WHEN** the user declines UAC
- **THEN** the operation returns a cancelled result without changing the driver

#### Scenario: Driver rank prevents binding
- **WHEN** the signed package is staged but the device remains bound to another driver
- **THEN** setup reports an incomplete binding with repair guidance rather than success

#### Scenario: Helper fails during repair
- **WHEN** the helper fails after the watchdog was paused
- **THEN** maintenance recovery resumes supervision and exposes the failed step
- **AND** unrelated driver packages are not deleted

#### Scenario: Reboot required
- **WHEN** Windows reports that binding requires a reboot or device reconnection
- **THEN** setup reports the pending action and does not claim verified readiness

### Requirement: Liveness Before Initial Printer Discovery

The HTTP server SHALL start independently of initial USB discovery. GET /health SHALL remain responsive while hardware is absent, uninitialized, reconnecting, or undergoing driver repair; printer readiness SHALL remain separate.

#### Scenario: Cold start without printer
- **WHEN** the application starts with no supported USB printer attached
- **THEN** GET /health returns success while printer readiness reports unavailable
- **AND** the watchdog does not restart the service solely for absent hardware

#### Scenario: Printer connected after startup
- **WHEN** a supported correctly bound printer is connected after cold start
- **THEN** readiness can transition to available without restarting the HTTP server
