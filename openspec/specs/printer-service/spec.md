# Printer Service Specification

## Purpose

Core service layer managing print operations with USB reconnection and background recovery.
## Requirements
### Requirement: Print Failure and Transport Recovery

The system SHALL execute each accepted print operation at most once after it begins writing, with one USB owner serializing whole print attempts and reconnects. Deterministic input errors and printer errors during a print attempt SHALL terminate that request and return an error to its requester. On a non-input printer error, the owner SHALL be released and background transport recovery SHALL continue for later requests. Discovery before a job begins may retry indefinitely. The pending-job channel SHALL hold at most 32 requests and reject excess work before output. Health reads SHALL remain available during recovery. A failed request may have emitted partial output; the service SHALL NOT claim exactly-once physical delivery.

#### Scenario: Print succeeds on first attempt
- **WHEN** the USB connection is stable and the printer accepts output
- **THEN** the operation completes with attempts=1 in PRINT_SUMMARY

#### Scenario: Ambiguous print failure returns without replay
- **WHEN** a print attempt fails with a non-input printer error
- **THEN** the owner releases the failed transport, returns the error to the requester, and does not replay the accepted command stream

#### Scenario: Background recovery after failed job
- **WHEN** a print attempt fails and releases its owner
- **THEN** the single owner continues discovery and can restore transport for later requests

#### Scenario: Discovery while disconnected
- **WHEN** no owner is available before a job begins
- **THEN** discovery continues with bounded backoff while status reads remain responsive

#### Scenario: Concurrent jobs and probes
- **WHEN** multiple jobs and a health refresh are pending
- **THEN** job attempts do not interleave and probes run only between attempts

#### Scenario: Queue capacity exceeded
- **WHEN** the 32 pending-job slots are occupied
- **THEN** a new submission is rejected before USB output and accepted jobs keep FIFO ordering

### Requirement: Print ID Generation

The system SHALL generate unique 8-character hex IDs for tracing print jobs across retries and reconnections.

**Implementation:** `src/services/printer.rs:21-28`

```rust
static PRINT_COUNTER: AtomicU32 = AtomicU32::new(0);

fn generate_print_id() -> String {
    let counter = PRINT_COUNTER.fetch_add(1, Ordering::SeqCst);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u32;
    format!("{:04x}{:04x}", timestamp & 0xFFFF, counter & 0xFFFF)
}
```

#### Scenario: Print ID format
- **WHEN** a new print operation starts
- **THEN** print_id is 8 hex characters: 4 chars timestamp (lower 16 bits) + 4 chars counter

#### Scenario: Print ID uniqueness
- **WHEN** multiple print jobs are submitted rapidly
- **THEN** atomic counter ensures unique IDs even with same timestamp

#### Scenario: Print ID in logs
- **WHEN** print operation executes
- **THEN** all related logs include `[print_id=XXXXXXXX]` for correlation

---

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

### Requirement: Command Execution Pipeline

The system SHALL execute commands sequentially with per-command logging and error propagation.

**Implementation:** `src/services/printer.rs:196-302`

```rust
async fn execute_commands_inner(
    driver: CustomUsbDriver,
    commands: Vec<Command>,
    print_id: String,
) -> Result<(), PrinterError> {
    let start = Instant::now();
    let cmd_count = commands.len();
    log::info!("[print_id={}] execute_commands: Starting {} commands...", print_id, cmd_count);

    let mut printer = Printer::new(driver, Protocol::default(), None);
    printer.init()?;

    for (idx, command) in commands.iter().enumerate() {
        let cmd_start = Instant::now();
        let cmd_name = /* resolve command name */;

        let result = match command {
            Command::Print(_) => printer.print(),
            Command::Bold(enabled) => printer.bold(*enabled),
            Command::Writeln(text) => printer.writeln(text),
            // ... all 40 command variants ...
        };

        match result {
            Ok(_) => log::debug!(
                "execute_commands: [{}/{}] {} OK in {:?}",
                idx + 1, cmd_count, cmd_name, cmd_start.elapsed()
            ),
            Err(e) => {
                log::error!(
                    "execute_commands: [{}/{}] {} FAILED after {:?}: {:?}",
                    idx + 1, cmd_count, cmd_name, cmd_start.elapsed(), e
                );
                return Err(e);
            }
        }
    }

    printer.print_cut()?;
    log::info!(
        "[print_id={}] execute_commands: COMPLETE - {} commands in {:?}",
        print_id, cmd_count, start.elapsed()
    );
    Ok(())
}
```

#### Scenario: All commands succeed
- **WHEN** all commands execute without error
- **THEN** final `print_cut()` is called and `Ok(())` returned

#### Scenario: Command fails mid-execution
- **WHEN** any command returns error
- **THEN** execution stops immediately and the error propagates to the requester without replaying the complete job

#### Scenario: Per-command timing
- **WHEN** each command executes
- **THEN** debug log shows `[idx/total] CommandName OK in duration`

---

### Requirement: Status Broadcasting

The service SHALL publish a structured health snapshot through a watch channel, with transport availability, readiness, busy state, reason, capability, generation, and observation timestamps. Existing boolean consumers SHALL receive a derived compatibility state. No status channel SHALL remain a supported configuration with publication treated as a no-op.

#### Scenario: Status channel connected
- **WHEN** health changes and a channel is configured
- **THEN** GUI and sensor consumers receive the same authoritative snapshot

#### Scenario: Status channel not connected
- **WHEN** no status channel is configured
- **THEN** health publication is a no-op without affecting printing

### Requirement: Test Print Execution

The system SHALL execute test prints with configurable patterns.

**Implementation:** `src/services/printer.rs:304-360`

```rust
pub async fn print_test(&self, request: PrinterTestSchema) -> Result<(), AppError> {
    self.with_retry(|driver, print_id| {
        let test_line = request.test_line().to_string();
        let test_page = request.test_page();
        async move {
            let mut printer = Printer::new(driver, Protocol::default(), None);
            printer.init()?;

            if test_page {
                // Full test pattern
                printer.smoothing(true)?;
                printer.bold(true)?;
                printer.underline(EscUnderlineMode::Single.into())?;
                printer.writeln("Bold underline")?;
                printer.justify(EscJustifyMode::Center.into())?;
                printer.reverse(true)?;
                printer.writeln("Hello world - Reverse")?;
                // ... more test patterns ...
            } else {
                printer.writeln(&test_line)?;
            }

            printer.print_cut()?;
            Ok(())
        }
    }).await
}
```

#### Scenario: Test page prints full pattern
- **WHEN** `test_page: true` in request
- **THEN** prints multiple formatting styles (bold, underline, reverse, sizes)

#### Scenario: Test line prints custom text
- **WHEN** `test_page: false` and `test_line` provided
- **THEN** prints only the custom line with cut

---

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

### Requirement: Consumer Command Parity in Reprints

Normal printing, API reprints, and GUI reprints SHALL use the same command validation and encoding semantics. Reprint midpoint calculation SHALL count each BitImage as one content-producing command in addition to the existing text and barcode commands. Legacy Italic commands SHALL NOT contribute visible content, emit bytes, or be restored around markers. Existing top/middle/bottom markers, other formatting restoration, recovery for later requests after transport errors, and omission of reprints from print history SHALL remain in effect. A failed print request SHALL NOT be replayed after an ambiguous printer I/O error.

#### Scenario: Image participates in midpoint
- **WHEN** the source receipt contains one BitImage followed by one Writeln
- **THEN** the middle marker is inserted between those two content commands
- **AND** markers also appear at the top and bottom

#### Scenario: Legacy italic reprint
- **WHEN** either reprint entry point processes a saved receipt with Italic commands
- **THEN** the receipt and markers contain no italic command bytes
- **AND** non-italic formatting is preserved

#### Scenario: Invalid GUI reprint
- **WHEN** a saved receipt fails preflight validation
- **THEN** the GUI receives an error without USB output or an infinite input-error retry
- **AND** no reprint history entry is added

## Design Decisions

### Recovery-First Architecture

The service is designed with a "recovery-first" mindset for retail POS reliability:

1. **Infinite Retry**: Never give up on recoverable failures
2. **Auto-Reconnect**: USB reconnection happens automatically on failure
3. **No Blocking**: HTTP API returns eventually (after retry succeeds)
4. **Status Broadcasting**: GUI always knows current printer state

### Print ID Tracing

Print IDs enable RCA by correlating logs across:
- Multiple retry attempts
- USB reconnection events
- Per-command execution
- Summary line for quick scanning

### Thread Safety

```
PrinterService
    └── Arc<Mutex<CustomUsbDriver>>
           └── Arc<Mutex<DeviceHandle>>
```

- Outer `Arc<Mutex>` allows driver replacement on reconnect
- Inner `Arc<Mutex>` (in driver) protects USB handle
- Service is `Clone` for sharing across async tasks

### Constants

| Constant | Value | Purpose |
|----------|-------|---------|
| USB retry interval | 5 seconds | Backoff between device open attempts |
| Health check interval | 30 seconds | Background connection verification |
