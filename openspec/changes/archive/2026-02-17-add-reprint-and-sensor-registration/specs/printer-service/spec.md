## ADDED Requirements

### Requirement: Reprint Command Execution with Anti-Fraud Markers

The system SHALL accept a reprint request containing an array of ESC/POS commands and execute them with reprint indicator markers injected at three positions: top, middle, and bottom of the receipt.

The system SHALL track ESC/POS formatting state (bold, underline, double_strike, reverse, justify, size, smoothing, flip, upside_down, font) by scanning commands up to the injection point. Before injecting each marker, the system SHALL reset formatting to defaults, print the marker, and then restore the tracked formatting state so subsequent original commands are unaffected.

The midpoint SHALL be calculated by counting content-producing commands (Write, Writeln, and all barcode/2D code commands) and splitting at `floor(count / 2)`. The split SHALL occur only at content-command boundaries, never between a formatting command and its corresponding content command.

Reprint markers SHALL use `Reverse(true)` (white-on-black) formatting and contain the text `** REPRINT COPY **`, the timestamp of the reprint, and the identifier `REIKA-escpos`.

Reprint operations SHALL use the same `with_retry` infinite retry loop as regular print operations.

Reprint operations SHALL NOT be logged to the print log.

#### Scenario: Reprint with formatting state preservation
- **WHEN** a reprint is requested for commands containing `Bold(true)` followed by `Writeln("TOTAL")` followed by `Bold(false)`
- **THEN** the system injects the middle marker between content commands, resets formatting before the marker, and restores `Bold(true)` after the marker so subsequent commands print correctly

#### Scenario: Reprint marker positions
- **WHEN** a reprint is requested for a receipt with 10 content-producing commands
- **THEN** the output contains a reprint marker before the first original command (top), between the 5th and 6th content commands (middle), and after the last original command before the cut (bottom)

#### Scenario: Reprint with Init/Reset in command stream
- **WHEN** the original commands contain an `Init` or `Reset` command at position N
- **THEN** the formatting state tracker resets all fields to defaults at position N, and any subsequent marker injection uses the correct post-reset state

### Requirement: Sensor Health Reporting

The system SHALL report its health state to the REIKA sensor monitoring endpoint (`POST /api/sensors/report`) when a valid API key and server URL are configured.

The system SHALL report the following state values:
- `ONLINE` when the printer is connected and the last health check passed
- `OFFLINE` when the printer is disconnected or USB device is not found
- `USB_ERROR` when a USB partial/zero write, stale handle, mutex lock failure, or interface claim failure occurs
- `PRINT_FAIL` when a print command execution fails

The system SHALL send heartbeat reports at a 60-second interval with the current state value. The REIKA server deduplicates heartbeats (same value only updates `last_seen_at`).

The system SHALL report state changes immediately upon occurrence, without waiting for the next heartbeat cycle.

The system SHALL authenticate with the REIKA server using the configured API key in the `X-Sensor-Key` HTTP header.

The system SHALL use HTTPS with certificate verification disabled (matching the ESP8266 firmware `setInsecure()` pattern for self-signed certificates).

If the REIKA server is unreachable, the system SHALL log a warning and retry on the next heartbeat cycle. Sensor reporting failures SHALL NOT block or affect printing operations.

The sensor reporter SHALL run as a background Tokio task, subscribing to the existing `watch::channel<bool>` for online/offline status and receiving critical error events via `tokio::sync::mpsc` from the printer service and USB driver.

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
