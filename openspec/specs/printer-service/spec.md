# Printer Service Specification

Core service layer managing print operations with automatic retry and USB reconnection.

## Requirements

### Requirement: Infinite Retry with Reconnection

The system SHALL retry print operations indefinitely, reconnecting USB after each failure.

**Implementation:** `src/services/printer.rs:114-157`

```rust
async fn with_retry<F, Fut, T>(&self, f: F) -> Result<T, AppError>
where
    F: Fn(CustomUsbDriver, String) -> Fut,
    Fut: Future<Output = Result<T, PrinterError>>,
{
    let start = Instant::now();
    let mut attempt = 0u32;
    let print_id = generate_print_id();

    log::info!("[print_id={}] Starting print operation...", print_id);

    loop {
        attempt += 1;
        let op_start = Instant::now();
        log::info!("[print_id={}] Attempt #{} starting...", print_id, attempt);

        let driver = self.driver.lock().await.clone();
        match f(driver, print_id.clone()).await {
            Ok(result) => {
                log::info!(
                    "[PRINT_SUMMARY] print_id={} | status=OK | attempts={} | duration={:?}",
                    print_id, attempt, start.elapsed()
                );
                return Ok(result);
            }
            Err(e) => {
                log::error!(
                    "[print_id={}] Attempt #{} FAILED after {:?}: {:?}",
                    print_id, attempt, op_start.elapsed(), e
                );
                log::info!("[print_id={}] Reconnecting before retry...", print_id);
                self.reconnect().await;
            }
        }
    }
}
```

#### Scenario: Print succeeds on first attempt
- **WHEN** USB connection is stable and printer ready
- **THEN** operation completes with `attempts=1` in `[PRINT_SUMMARY]` log

#### Scenario: Print succeeds after USB reconnection
- **WHEN** first attempt fails due to USB error
- **THEN** service reconnects USB, retries, and eventually succeeds with `attempts > 1`

#### Scenario: Continuous retry on persistent failure
- **WHEN** printer remains disconnected
- **THEN** service retries indefinitely (never returns error to client until success)

---

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

The system SHALL replace the USB driver with a fresh connection on failure.

**Implementation:** `src/services/printer.rs:104-113`

```rust
async fn reconnect(&self) {
    let start = Instant::now();
    self.update_status(false);
    log::info!("reconnect: Starting USB reconnection...");
    let new_driver = Self::initialize_device_with_config(&self.usb_config).await;
    let mut driver = self.driver.lock().await;
    *driver = new_driver;
    self.update_status(true);
    log::info!("reconnect: USB reconnected in {:?}", start.elapsed());
}
```

#### Scenario: Reconnection broadcasts offline status
- **WHEN** reconnection starts
- **THEN** `status_tx.send(false)` notifies GUI of offline state

#### Scenario: Reconnection replaces driver
- **WHEN** new USB connection established
- **THEN** old driver is dropped and new driver replaces it in `Arc<Mutex>`

#### Scenario: Reconnection broadcasts online status
- **WHEN** reconnection completes
- **THEN** `status_tx.send(true)` notifies GUI of online state

---

### Requirement: Initialize Device with Infinite Retry

The system SHALL attempt USB device initialization indefinitely with 5-second backoff.

**Implementation:** `src/services/printer.rs:58-82`

```rust
pub async fn initialize_device_with_config(config: &UsbConfig) -> CustomUsbDriver {
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        log::info!(
            "USB init attempt #{}: VID=0x{:04X}, PID=0x{:04X}",
            attempt, config.vendor_id, config.product_id
        );
        match CustomUsbDriver::open(config) {
            Ok(driver) => {
                log::info!("USB device opened successfully on attempt #{}", attempt);
                return driver;
            }
            Err(e) => {
                log::warn!(
                    "USB init attempt #{} failed: {:?}. Retrying in 5 seconds...",
                    attempt, e
                );
                sleep(Duration::from_secs(5)).await;
            }
        }
    }
}
```

#### Scenario: Device found immediately
- **WHEN** USB printer is connected and ready
- **THEN** returns driver on attempt #1

#### Scenario: Device found after waiting
- **WHEN** USB printer is connected after service starts
- **THEN** service waits (5s intervals) until device appears, then returns driver

#### Scenario: Logs each attempt
- **WHEN** attempting to open device
- **THEN** logs VID, PID, attempt number, and result for each attempt

---

### Requirement: Non-Blocking Device Probe

The system SHALL provide a non-blocking `try_open` for initial connection attempts.

**Implementation:** `src/services/printer.rs:84-102`

```rust
pub fn try_open(config: &UsbConfig) -> Option<CustomUsbDriver> {
    log::debug!(
        "try_open: VID=0x{:04X}, PID=0x{:04X}, EP={:?}, IF={:?}",
        config.vendor_id, config.product_id, config.endpoint, config.interface
    );
    match CustomUsbDriver::open(config) {
        Ok(driver) => {
            log::info!("try_open: USB device opened successfully");
            Some(driver)
        }
        Err(e) => {
            log::debug!("try_open: USB open failed: {:?}", e);
            None
        }
    }
}
```

#### Scenario: Device available
- **WHEN** USB device is connected
- **THEN** returns `Some(driver)` immediately

#### Scenario: Device unavailable
- **WHEN** USB device is not connected
- **THEN** returns `None` without blocking (no retry)

---

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
- **THEN** execution stops immediately, error propagates to trigger retry/reconnect

#### Scenario: Per-command timing
- **WHEN** each command executes
- **THEN** debug log shows `[idx/total] CommandName OK in duration`

---

### Requirement: Status Broadcasting

The system SHALL broadcast printer online/offline status via watch channel.

**Implementation:** `src/services/printer.rs:46-55`

```rust
pub struct PrinterService {
    driver: Arc<Mutex<CustomUsbDriver>>,
    usb_config: UsbConfig,
    status_tx: Option<watch::Sender<bool>>,
}

fn update_status(&self, online: bool) {
    if let Some(tx) = &self.status_tx {
        let _ = tx.send(online);
    }
}

pub fn with_status(mut self, status_tx: watch::Sender<bool>) -> Self {
    self.status_tx = Some(status_tx);
    self
}
```

#### Scenario: Status channel connected
- **WHEN** service created with `with_status(tx)`
- **THEN** all status changes broadcast to GUI via watch channel

#### Scenario: Status channel not connected
- **WHEN** service created without status channel
- **THEN** `update_status` is no-op (optional functionality)

---

### Requirement: Health Check via Init Command

The system SHALL verify printer connectivity by sending an init command.

**Implementation:** `src/services/printer.rs:159-194`

```rust
pub async fn check_connection(&self) -> bool {
    log::debug!("check_connection: Health check starting...");
    let driver = self.driver.lock().await.clone();
    match Self::try_init(driver).await {
        Ok(()) => {
            self.update_status(true);
            log::debug!("check_connection: Health check result = true");
            true
        }
        Err(e) => {
            log::debug!("check_connection: Health check failed: {:?}", e);
            self.update_status(false);
            false
        }
    }
}

async fn try_init(driver: CustomUsbDriver) -> Result<(), PrinterError> {
    let start = Instant::now();
    let mut printer = Printer::new(driver, Protocol::default(), None);
    printer.init()
}
```

#### Scenario: Health check succeeds
- **WHEN** printer responds to init command
- **THEN** returns `true` and broadcasts online status

#### Scenario: Health check fails
- **WHEN** printer does not respond (USB error)
- **THEN** returns `false` and broadcasts offline status

---

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
