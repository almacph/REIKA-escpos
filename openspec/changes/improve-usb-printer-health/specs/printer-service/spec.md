## REMOVED Requirements

### Requirement: Health Check via Init Command
**Reason**: Init changes printer state and a successful write does not verify physical readiness.
**Migration**: Use the cached structured snapshot and serialized non-destructive probes; retain Init for actual print jobs only.

## MODIFIED Requirements

### Requirement: Infinite Retry with Reconnection

The system SHALL retry accepted print operations indefinitely for recoverable USB failures, with one USB owner serializing whole print attempts and reconnects. Deterministic input errors SHALL terminate without retry. The pending-job channel SHALL hold at most 32 requests and reject excess work before output. Health reads SHALL remain available during retry. Whole-job retry SHALL NOT be described as exactly-once delivery.

#### Scenario: Print succeeds on first attempt
- **WHEN** the USB connection is stable and the printer accepts output
- **THEN** the operation completes with attempts=1 in PRINT_SUMMARY

#### Scenario: Print succeeds after USB reconnection
- **WHEN** an attempt fails with a recoverable USB error
- **THEN** the single owner reconnects and retries, reporting attempts greater than 1 on success

#### Scenario: Continuous retry on persistent failure
- **WHEN** an accepted job waits for a disconnected printer
- **THEN** recovery continues indefinitely with bounded backoff while status reads remain responsive

#### Scenario: Concurrent jobs and probes
- **WHEN** multiple jobs and a health refresh are pending
- **THEN** job attempts do not interleave and probes run only between attempts

#### Scenario: Queue capacity exceeded
- **WHEN** the 32 pending-job slots are occupied
- **THEN** a new submission is rejected before USB output and accepted jobs keep FIFO ordering

### Requirement: USB Reconnection

The single USB owner SHALL invalidate a failed connection generation and release all old device handles before opening a replacement. It SHALL recover failed idle connections as well as active-job failures and SHALL reject observations from obsolete generations. Driver/access errors SHALL expose setup guidance rather than trigger concurrent reopen loops.

#### Scenario: Reconnection broadcasts offline status
- **WHEN** reconnection starts
- **THEN** consumers receive reconnecting/unavailable status and the compatibility boolean is false

#### Scenario: Reconnection replaces driver
- **WHEN** a replacement is opened
- **THEN** the previous handle has already been released and the new connection has a distinct generation

#### Scenario: Reconnection broadcasts online status
- **WHEN** a new connection is successfully opened and reconciled
- **THEN** transport availability is broadcast without claiming unverified physical readiness

#### Scenario: Idle recovery
- **WHEN** the printer is unplugged and later reattached without a print request
- **THEN** the worker detects the change and restores the transport without restarting the app

### Requirement: Initialize Device with Infinite Retry

The system SHALL attempt initialization until a supported selected device becomes available or the application shuts down. Retry delays SHALL follow 1, 2, 5, 10, then 30 seconds with at most 10 percent positive jitter. Valid arrival events MAY wake discovery earlier. Driver/access failures SHALL use the slow retry cadence and show repair guidance. Successful connection SHALL reset backoff.

#### Scenario: Device found immediately
- **WHEN** the selected USB printer is available
- **THEN** the first initialization attempt opens it

#### Scenario: Device found after waiting
- **WHEN** the printer is attached after startup
- **THEN** the owner discovers it through bounded-backoff retries or a supported arrival event

#### Scenario: Logs each attempt
- **WHEN** initialization is attempted
- **THEN** diagnostic logs identify VID, PID, attempt number, error category, and result

#### Scenario: Shutdown during retry
- **WHEN** the application exits during discovery
- **THEN** no further retries are scheduled and owned resources are released

### Requirement: Non-Blocking Device Probe

Initial discovery and native USB operations SHALL execute on a dedicated blocking worker, not the HTTP or GUI execution threads. A probe SHALL report a finite attempt result to the owner rather than contain its own indefinite retry loop. Recovery scheduling SHALL remain the owner's responsibility.

#### Scenario: Device available
- **WHEN** the worker probes a supported available device
- **THEN** it reports an opened connection without blocking HTTP or GUI status reads

#### Scenario: Device unavailable
- **WHEN** no selected device is available
- **THEN** the attempt reports absence and the owner schedules later discovery

### Requirement: Status Broadcasting

The service SHALL publish a structured health snapshot through a watch channel, with transport availability, readiness, busy state, reason, capability, generation, and observation timestamps. Existing boolean consumers SHALL receive a derived compatibility state. No status channel SHALL remain a supported configuration with publication treated as a no-op.

#### Scenario: Status channel connected
- **WHEN** health changes and a channel is configured
- **THEN** GUI and sensor consumers receive the same authoritative snapshot

#### Scenario: Status channel not connected
- **WHEN** no status channel is configured
- **THEN** health publication is a no-op without affecting printing

### Requirement: Sensor Health Reporting

The system SHALL report its health state to the REIKA sensor monitoring endpoint (`POST /api/sensors/report`) when a valid API key and server URL are configured.

The system SHALL report the following state values:
- `ONLINE` when transport evidence is fresh and available with no known blocking physical fault; unsupported physical readiness alone does not mean offline
- `OFFLINE` when transport is unavailable or stale, or a verified physical fault blocks printing
- `USB_ERROR` when a USB partial/zero write, stale handle, mutex lock failure, or interface claim failure occurs
- `PRINT_FAIL` when a print command execution fails

The reporter SHALL initialize from the current health snapshot and send heartbeat reports at a 60-second interval with the current mapped state value, without resetting the timer on status events. The REIKA server deduplicates heartbeats (same value only updates `last_seen_at`).

The system SHALL report state changes immediately upon occurrence, without waiting for the next heartbeat cycle.

The system SHALL authenticate with the REIKA server using the configured API key in the `X-Sensor-Key` HTTP header.

The system SHALL use HTTPS with certificate verification disabled (matching the ESP8266 firmware `setInsecure()` pattern for self-signed certificates).

If the REIKA server is unreachable, the system SHALL log a warning and retry on the next heartbeat cycle. Sensor reporting failures SHALL NOT block or affect printing operations.

The sensor reporter SHALL run as a background Tokio task, subscribing to the authoritative structured health snapshot and its compatibility mapping and receiving critical error events via `tokio::sync::mpsc` from the printer service and USB driver.

#### Scenario: Heartbeat reporting while online
- **WHEN** the printer is connected and 60 seconds have elapsed since the last report
- **THEN** the system sends `{ "value": "ONLINE" }` to the REIKA sensor endpoint with the API key in the `X-Sensor-Key` header

#### Scenario: Immediate USB error reporting
- **WHEN** the USB driver detects a partial write (0 of N bytes written)
- **THEN** the system immediately sends `{ "value": "USB_ERROR" }` to the REIKA sensor endpoint without waiting for the heartbeat timer

#### Scenario: Printer goes offline
- **WHEN** the printer status changes from online to offline
- **THEN** the system immediately sends `{ "value": "OFFLINE" }` to the REIKA sensor endpoint

#### Scenario: Recovery after error
- **WHEN** the printer reconnects successfully after a USB_ERROR or OFFLINE state
- **THEN** the system immediately sends `{ "value": "ONLINE" }` to the REIKA sensor endpoint

#### Scenario: REIKA server unreachable
- **WHEN** the sensor report HTTP request fails (timeout, connection refused, etc.)
- **THEN** the system logs a warning with the error details and continues normal operation without retrying until the next heartbeat cycle

#### Scenario: No API key configured
- **WHEN** the `reika.api_key` configuration is empty or not set
- **THEN** the sensor reporter does not start and no reports are sent

The reporter SHALL retain the existing four wire values; unsupported readiness alone SHALL NOT produce USB_ERROR. Fresh recovery observations SHALL replace prior transient error state.

#### Scenario: Heartbeat amid repeated observations
- **WHEN** status observations occur more frequently than 60 seconds
- **THEN** heartbeat scheduling is not reset or starved

#### Scenario: Error followed by recovery
- **WHEN** USB_ERROR or PRINT_FAIL is followed by a fresh available transport observation with no known blocking fault
- **THEN** the reporter publishes ONLINE rather than retaining the old error indefinitely

## ADDED Requirements

### Requirement: Non-Destructive Bounded Printer Health

Health checks SHALL NOT initialize, reset, feed, cut, pulse a drawer, or print. Idle presence checks SHALL run every 5 seconds. Verified readiness profiles SHALL use idle-only request/response transactions bounded to 250 ms each and 1 second per cycle, scheduled every 15 seconds. Unverified or output-only profiles SHALL report readiness unknown without sending speculative status commands. Three consecutive optional status failures SHALL disable detailed probing for the connection until reconnect or explicit retest, without alone declaring USB absent.

#### Scenario: Status request while printing
- **WHEN** a receipt is executing
- **THEN** status readers use the snapshot and no probe bytes enter the receipt stream

#### Scenario: Unsupported status protocol
- **WHEN** a device has no verified status capability
- **THEN** transport can be available while physical readiness is unknown

#### Scenario: Paper or cover fault
- **WHEN** a verified response reports paper-out or cover-open
- **THEN** the fault is exposed without restarting the process or rebinding the driver

### Requirement: Health Freshness and Failure Classification

The service SHALL treat transport observations older than 15 seconds as stale and physical readiness observations older than 30 seconds as unknown. Successful print writes SHALL refresh transport evidence. Two consecutive soft transport-probe failures SHALL mark transport unavailable; definite removal, NoDevice, or failed/partial print writes SHALL invalidate the connection immediately. Old-generation responses SHALL NOT overwrite current state.

#### Scenario: Slow unsupported status response
- **WHEN** an optional status query times out while transport remains available
- **THEN** readiness becomes unknown without an immediate disconnected transition

#### Scenario: Hard disconnect
- **WHEN** the device is removed or a write reports NoDevice
- **THEN** its generation becomes unavailable immediately and one recovery sequence starts

#### Scenario: Stale response after recovery
- **WHEN** a result belongs to an older generation
- **THEN** it is discarded without changing the current snapshot
