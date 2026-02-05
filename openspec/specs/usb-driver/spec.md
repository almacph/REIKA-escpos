# USB Driver Specification

Custom USB driver implementation for ESC/POS thermal printer communication.

## Requirements

### Requirement: USB Device Discovery

The system SHALL enumerate USB devices and locate the thermal printer by Vendor ID and Product ID.

**Implementation:** `src/services/usb_driver.rs:60-95`

```rust
let devices = context.devices()?;
for device in devices.iter() {
    let device_descriptor = device.device_descriptor()?;
    if device_descriptor.vendor_id() == config.vendor_id
        && device_descriptor.product_id() == config.product_id
    {
        // Found matching device
    }
}
```

#### Scenario: Device found by VID/PID
- **WHEN** USB device with matching VID (0x0483) and PID (0x5840) is connected
- **THEN** the driver returns a valid `CustomUsbDriver` instance

#### Scenario: Device not found
- **WHEN** no USB device matches the configured VID/PID
- **THEN** the driver returns `PrinterError::Io("USB device not found")`

---

### Requirement: Endpoint Auto-Discovery

The system SHALL automatically discover bulk IN and OUT endpoints when manual configuration is not provided.

**Implementation:** `src/services/usb_driver.rs:97-135`

```rust
fn discover_endpoints(config_descriptor: &ConfigDescriptor) -> Result<(u8, u8, u8)> {
    config_descriptor
        .interfaces()
        .flat_map(|interface| interface.descriptors())
        .flat_map(|descriptor| {
            let interface_number = descriptor.interface_number();
            let mut input_endpoint = None;
            let mut output_endpoint = None;

            for endpoint in descriptor.endpoint_descriptors() {
                if endpoint.transfer_type() == TransferType::Bulk {
                    match endpoint.direction() {
                        Direction::In => input_endpoint = Some(endpoint.address()),
                        Direction::Out => output_endpoint = Some(endpoint.address()),
                    }
                }
            }
            match (output_endpoint, input_endpoint) {
                (Some(out), Some(inp)) => Some((out, inp, interface_number)),
                _ => None,
            }
        })
        .next()
        .ok_or_else(|| PrinterError::Io("no suitable endpoints found".to_string()))
}
```

#### Scenario: Auto-discovery finds bulk endpoints
- **WHEN** USB device has bulk IN and OUT endpoints
- **THEN** the driver uses discovered endpoint addresses (e.g., OUT=0x01, IN=0x81)

#### Scenario: Manual endpoint override
- **WHEN** `config.endpoint` and `config.interface` are provided
- **THEN** the driver uses `out_ep = config.endpoint` and `in_ep = config.endpoint | 0x80`

---

### Requirement: Interface Claiming with Retry

The system SHALL claim the USB interface with up to 5 retry attempts, waiting 100ms between attempts.

**Implementation:** `src/services/usb_driver.rs:150-175`

```rust
let mut claim_result = device_handle.claim_interface(interface_number);
let mut claim_attempts = 0;
const MAX_CLAIM_ATTEMPTS: u32 = 5;

while claim_result.is_err() && claim_attempts < MAX_CLAIM_ATTEMPTS {
    claim_attempts += 1;
    log::debug!(
        "claim_interface attempt {}/{} failed, retrying in 100ms...",
        claim_attempts, MAX_CLAIM_ATTEMPTS
    );
    std::thread::sleep(Duration::from_millis(100));
    claim_result = device_handle.claim_interface(interface_number);
}
```

#### Scenario: Interface claimed on first attempt
- **WHEN** USB interface is available
- **THEN** the driver claims the interface immediately

#### Scenario: Interface claimed after retry
- **WHEN** USB interface is temporarily busy (Windows resource release delay)
- **THEN** the driver retries up to 5 times with 100ms delays and succeeds

#### Scenario: Interface claim fails after max attempts
- **WHEN** interface cannot be claimed after 5 attempts
- **THEN** the driver returns `PrinterError::Io` with claim failure details

---

### Requirement: Partial Write Detection

The system SHALL treat partial or zero USB writes as errors to prevent silent print failures.

**Implementation:** `src/services/usb_driver.rs:256-272`

```rust
fn write(&self, data: &[u8]) -> Result<()> {
    let result = device.write_bulk(self.output_endpoint, data, self.timeout);

    match &result {
        Ok(bytes_written) => {
            if *bytes_written != data.len() {
                log::error!(
                    "[PRINT_FAILURE] USB partial write: wrote {}/{} bytes",
                    bytes_written, data.len()
                );
                return Err(PrinterError::Io(format!(
                    "USB write incomplete: expected {} bytes, wrote {} bytes",
                    data.len(), bytes_written
                )));
            }
        }
        Err(e) => { /* log and return error */ }
    }
    Ok(())
}
```

#### Scenario: Complete write succeeds
- **WHEN** USB write transfers all bytes (bytes_written == data.len())
- **THEN** the operation returns `Ok(())`

#### Scenario: Partial write detected as error
- **WHEN** USB write transfers fewer bytes than requested (0 < bytes_written < data.len())
- **THEN** the driver returns `PrinterError::Io("USB write incomplete")` triggering reconnection

#### Scenario: Zero-byte write detected as error
- **WHEN** USB write transfers zero bytes (stale handle after power cycle)
- **THEN** the driver returns error instead of false success

---

### Requirement: USB Operation Timeout

The system SHALL enforce a 5-second timeout on all USB bulk transfer operations.

**Implementation:** `src/services/usb_driver.rs:20,240`

```rust
const DEFAULT_TIMEOUT_SECONDS: u64 = 5;

// In write():
let result = device.write_bulk(self.output_endpoint, data, self.timeout);

// In constructor:
timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECONDS),
```

#### Scenario: Write completes within timeout
- **WHEN** USB device responds within 5 seconds
- **THEN** the write operation succeeds normally

#### Scenario: Write times out
- **WHEN** USB device does not respond within 5 seconds
- **THEN** `rusb::Error::Timeout` is returned and converted to `PrinterError::Io`

---

### Requirement: Thread-Safe Driver Access

The system SHALL wrap the USB device handle in `Arc<Mutex>` for thread-safe access from async contexts.

**Implementation:** `src/services/usb_driver.rs:25-35`

```rust
#[derive(Clone)]
pub struct CustomUsbDriver {
    vendor_id: u16,
    product_id: u16,
    output_endpoint: u8,
    input_endpoint: u8,
    device: Arc<Mutex<DeviceHandle<Context>>>,
    timeout: Duration,
}
```

#### Scenario: Concurrent access from multiple tasks
- **WHEN** multiple async tasks attempt USB operations
- **THEN** mutex serializes access preventing data corruption

#### Scenario: Driver cloning for service
- **WHEN** `PrinterService` needs to share driver across tasks
- **THEN** `Clone` derives shallow copy with shared `Arc<Mutex>` reference

---

### Requirement: Endpoint Halt Clearing

The system SHALL clear endpoint halt status on connection to recover from stale data toggles.

**Implementation:** `src/services/usb_driver.rs:180-188`

```rust
if let Err(e) = device_handle.clear_halt(output_endpoint) {
    log::debug!(
        "clear_halt on endpoint 0x{:02X} returned: {} (non-fatal)",
        output_endpoint, e
    );
}
```

#### Scenario: Clear halt succeeds
- **WHEN** endpoint was previously halted
- **THEN** data toggle bits are reset allowing normal communication

#### Scenario: Clear halt fails (non-fatal)
- **WHEN** endpoint clear_halt returns error (e.g., not halted)
- **THEN** error is logged at debug level and connection continues

---

### Requirement: Instrumented USB Operations

The system SHALL log timing and context for all USB operations to enable RCA (Root Cause Analysis).

**Implementation:** `src/services/usb_driver.rs:230-295`

```rust
fn write(&self, data: &[u8]) -> Result<()> {
    let start = Instant::now();
    log::info!(
        "USB write START: {} bytes to endpoint 0x{:02X}, timeout={}ms",
        data.len(), self.output_endpoint, self.timeout.as_millis()
    );

    let lock_start = Instant::now();
    let device = self.device.lock().map_err(|e| {
        log::error!("USB mutex lock FAILED after {:?}: {}", lock_start.elapsed(), e);
        PrinterError::Io(e.to_string())
    })?;
    log::debug!("USB mutex acquired in {:?}", lock_start.elapsed());

    let write_start = Instant::now();
    // ... perform write ...

    log::info!(
        "USB write OK: {}/{} bytes in {:?} (total {:?})",
        bytes_written, data.len(), write_start.elapsed(), start.elapsed()
    );
}
```

#### Scenario: Successful write with timing
- **WHEN** USB write completes successfully
- **THEN** logs include: bytes transferred, endpoint, lock duration, write duration, total duration

#### Scenario: Failed write with context
- **WHEN** USB write fails
- **THEN** logs include: `[PRINT_FAILURE]` tag, endpoint, elapsed time, error details, rusb error kind

---

## Design Decisions

### Why Custom Driver Instead of escpos Crate's Driver

The `escpos` crate provides a USB driver, but we use a custom implementation for:

1. **Partial Write Detection**: The escpos driver treats any `write_bulk` success as complete. Our driver verifies `bytes_written == data.len()`.

2. **Detailed Logging**: Custom logging with timing, endpoint info, and `[PRINT_FAILURE]` tags for RCA.

3. **Interface Claim Retry**: Windows USB resource release can be delayed; retry logic handles this.

4. **Thread Safety Pattern**: Our `Arc<Mutex>` wrapping integrates with Tokio async runtime.

### Constants

| Constant | Value | Purpose |
|----------|-------|---------|
| `DEFAULT_TIMEOUT_SECONDS` | 5 | USB operation timeout |
| `MAX_CLAIM_ATTEMPTS` | 5 | Interface claim retries |
| Claim retry delay | 100ms | Time between claim attempts |
