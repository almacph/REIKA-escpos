# Root Cause Analysis: Printer False Success Response

**Date:** 2026-02-05
**Severity:** High
**Affected Component:** USB Driver / Print Service
**Issue ID:** RCA-2026-02-05-001

---

## Issue Description

The REIKA ESC/POS printer service reports successful print operations to the frontend when no actual printing occurred. The thermal printer did not feed paper despite the service returning a success response. This creates a critical mismatch between reported status and actual hardware behavior.

---

## Timeline of Events

### Problematic Session (07:41:56 - 07:45:28)
1. **07:41:56** - Service started, USB device opened successfully
2. **07:42:10** - First print operation executed successfully (actual print occurred)
3. **07:44:36** - Second print operation started
4. **07:44:36.617** - **Pipe error** on USB write (first failure indicator)
5. **07:44:36 - 07:45:08** - 8 retry attempts, all failing with:
   - Pipe errors
   - Timeouts (5 second USB write timeouts)
   - I/O errors
6. **07:45:08.983** - USB device physically disconnected/not found
7. **07:45:28.593** - New print operation started
8. **07:45:28.594** - **Critical Bug Manifests**: USB writes return `0 bytes written` but are treated as success

### Working Session (07:47:10 onward - after taskkill)
- Service restarted fresh
- All USB writes show proper byte counts (e.g., `USB write OK: 2/2 bytes`, `USB write OK: 3/3 bytes`)
- Print operations completed successfully with actual paper output

---

## Root Cause Analysis

### Primary Root Cause: Partial Write Not Treated as Error

**File:** `src/services/usb_driver.rs:225-281`

The USB driver's `write()` function has a critical flaw in its success path handling:

```rust
fn write(&self, data: &[u8]) -> Result<()> {
    // ...
    let result = device.write_bulk(self.output_endpoint, data, self.timeout);

    match &result {
        Ok(bytes_written) => {
            log::info!(
                "USB write OK: {}/{} bytes in {:?} (total {:?})",
                bytes_written,
                data.len(),
                write_start.elapsed(),
                start.elapsed()
            );
            if *bytes_written != data.len() {
                log::warn!(  // <-- ONLY LOGS WARNING, DOES NOT RETURN ERROR
                    "USB partial write: expected {} bytes, wrote {} bytes",
                    data.len(),
                    bytes_written
                );
            }
        }
        // ...
    }

    result.map_err(...)?;  // Only errors on rusb errors, not partial writes
    Ok(())  // Returns Ok even when 0 bytes written!
}
```

**Evidence from logs:**
```
[07:45:28.594] USB write OK: 0/2 bytes in 80.7µs
[07:45:28.594] WARN: USB partial write: expected 2 bytes, wrote 0 bytes
[07:45:28.594] USB write OK: 0/3 bytes in 118.4µs
[07:45:28.594] WARN: USB partial write: expected 3 bytes, wrote 0 bytes
...
[07:45:28.595] execute_commands: COMPLETE - 4 commands in 1.3166ms
[07:45:28.595] with_retry: SUCCESS on attempt #1 in 1.3372ms
```

The function returns `Ok(())` even when `bytes_written == 0`, causing the entire print command chain to be marked as successful.

### Secondary Issue: Stale USB Handle State After Power Cycle

**User Action:** Between retry attempts #5 and #6 (approximately 07:44:56 - 07:45:01), the user manually power-cycled the printer in an attempt to restore connectivity.

After this power cycle, the driver successfully reopened the USB connection but the device handle entered a degraded state where:
- `write_bulk()` returned `Ok(0)` instead of an error
- The rusb library did not report an error condition
- The physical USB endpoint was not accepting data
- The host USB controller maintained stale endpoint state from before the power cycle

This is a known issue with USB device power cycling while the host maintains an open handle. The USB host controller believes it still has a valid connection, but the device has reset its endpoint buffers and is no longer synchronized with the host's data toggle bits. The result is that writes "succeed" from the host's perspective (no error returned) but 0 bytes are actually transferred.

### Contributing Factor: Health Check Passes Despite Broken State

**File:** `src/services/printer.rs:143-162`

The `check_connection()` health check only calls `printer.init()` which may buffer commands internally and return success without actually writing to USB:

```rust
pub async fn check_connection(&self) -> bool {
    let driver = self.driver.lock().await.clone();
    match Self::try_init(driver).await {
        Ok(()) => {
            self.update_status(true);  // Reports healthy even when USB is broken
            true
        }
        // ...
    }
}
```

---

## Evidence Summary

### Failed Print Session (No Output)
| Timestamp | Event | Bytes Written | Expected |
|-----------|-------|---------------|----------|
| 07:45:28.594 | USB write | 0 | 2 |
| 07:45:28.594 | USB write | 0 | 3 |
| 07:45:28.594 | USB write | 0 | 15 |
| 07:45:28.594 | USB write | 0 | 3 |
| 07:45:28.594 | USB write | 0 | 3 |
| 07:45:28.595 | USB write | 0 | 4 |
| 07:45:28.595 | USB write | 0 | 4 |
| **Result** | **SUCCESS reported** | **0 total** | **30+** |

### Successful Print Session (After Restart)
| Timestamp | Event | Bytes Written | Expected |
|-----------|-------|---------------|----------|
| 07:47:14.118 | USB write | 2 | 2 |
| 07:47:14.119 | USB write | 3 | 3 |
| 07:47:14.120 | USB write | 15 | 15 |
| 07:47:14.120 | USB write | 3 | 3 |
| 07:47:14.121 | USB write | 3 | 3 |
| 07:47:14.121 | USB write | 4 | 4 |
| 07:47:14.122 | USB write | 4 | 4 |
| **Result** | **SUCCESS** | **34** | **34** |

---

## Recommended Changes

### 1. Treat Partial/Zero Writes as Errors (Critical)

**File:** `src/services/usb_driver.rs`
**Function:** `write()`

Change the partial write handling from a warning to an error:

```rust
// Current (problematic):
if *bytes_written != data.len() {
    log::warn!(...);
}
result.map_err(...)?;
Ok(())

// Recommended:
if *bytes_written != data.len() {
    log::error!(
        "USB write incomplete: wrote {}/{} bytes - treating as failure",
        bytes_written,
        data.len()
    );
    return Err(PrinterError::Io(format!(
        "USB write incomplete: expected {} bytes, wrote {} bytes",
        data.len(),
        bytes_written
    )));
}
```

### 2. Add Verification Write to Health Check

**File:** `src/services/printer.rs`
**Function:** `check_connection()`

Add an actual USB write verification instead of relying only on `init()`:

```rust
async fn try_init(driver: CustomUsbDriver) -> Result<(), PrinterError> {
    let mut printer = Printer::new(driver, Protocol::default(), None);
    printer.init()?;
    // Add: Verify USB is actually responsive with a real write
    // Could send a status request command (DLE EOT) and verify response
    Ok(())
}
```

### 3. USB Handle Reset on Reconnect

**File:** `src/services/usb_driver.rs`
**Function:** `open()`

After claiming the interface, perform a USB endpoint clear/reset to ensure clean state:

```rust
// After claim_interface succeeds:
device_handle.clear_halt(output_endpoint).ok(); // Clear any stall condition
// Optionally: device_handle.reset_device() for full reset
```

### 4. Add Connection Validation Before Print

**File:** `src/services/printer.rs`
**Function:** `with_retry()`

Before attempting print, validate the USB connection is truly healthy:

```rust
// After reconnect, verify with a test write
let test_result = driver.write(&[0x1B, 0x40]); // ESC @ (init command)
if test_result.is_err() {
    continue; // Reconnect failed, try again
}
```

---

## Files Requiring Modification

1. **`src/services/usb_driver.rs`**
   - Line ~253-259: Convert partial write warning to error
   - Line ~115-130: Add endpoint clear after interface claim

2. **`src/services/printer.rs`**
   - Line ~143-162: Enhance health check with actual write verification
   - Line ~90-99: Add connection validation after reconnect

---

## Impact Assessment

- **User Impact:** Users receive false success confirmations; receipts/orders may be silently lost
- **Business Impact:** Critical for POS systems where missing receipts cause operational issues
- **Data Integrity:** No data loss, but operational reliability severely compromised

---

## Testing Recommendations

1. Simulate USB disconnect during print operation
2. Test with USB hub power cycling
3. Verify error response when printer is powered off mid-print
4. Load test with rapid print requests to stress reconnection logic

---

## Logging Improvements

To catch this class of issues earlier and enable faster diagnosis, implement the following logging enhancements:

### 1. Elevate Partial Write Logs to ERROR Level

**Current behavior:** Partial writes logged as `WARN`, easily missed in log noise.

**Recommended:** Log partial/zero writes as `ERROR` with clear failure indicator:

```rust
// In usb_driver.rs write()
if *bytes_written != data.len() {
    log::error!(
        "[PRINT_FAILURE] USB partial write: wrote {}/{} bytes | endpoint=0x{:02X} | elapsed={:?}",
        bytes_written,
        data.len(),
        self.output_endpoint,
        write_start.elapsed()
    );
}
```

### 2. Add Print Operation Correlation IDs

Track each print job with a unique ID through the entire lifecycle:

```rust
// Generate at request entry point
let print_id = uuid::Uuid::new_v4().to_string()[..8].to_string();

// Include in all related logs
log::info!("[print_id={}] Starting print operation with {} commands", print_id, cmd_count);
log::info!("[print_id={}] USB write: {}/{} bytes", print_id, bytes_written, data.len());
log::info!("[print_id={}] Print operation result: {}", print_id, if success { "SUCCESS" } else { "FAILED" });
```

### 3. Add Cumulative Byte Counter Per Print Job

Track total bytes expected vs actually written for each print operation:

```rust
// In execute_commands_inner or with_retry
let mut total_bytes_expected: usize = 0;
let mut total_bytes_written: usize = 0;

// After each write, accumulate
total_bytes_expected += data.len();
total_bytes_written += bytes_written;

// At operation end, log summary
log::info!(
    "[print_id={}] Print complete: {}/{} bytes written ({}%)",
    print_id,
    total_bytes_written,
    total_bytes_expected,
    (total_bytes_written * 100) / total_bytes_expected.max(1)
);

// Flag suspicious completions
if total_bytes_written == 0 && total_bytes_expected > 0 {
    log::error!(
        "[print_id={}] [ZERO_WRITE_ANOMALY] Print reported success but 0 bytes transferred!",
        print_id
    );
}
```

### 4. Log USB State After Reconnect

Capture device state information after each reconnection attempt:

```rust
// After successful reconnect in printer.rs
log::info!(
    "reconnect: USB reconnected | attempt={} | device_state={{ vid=0x{:04X}, pid=0x{:04X}, ep_out=0x{:02X} }} | elapsed={:?}",
    attempt,
    config.vendor_id,
    config.product_id,
    output_endpoint,
    start.elapsed()
);

// Add verification write result
let verify_result = driver.write(&[0x1B, 0x40]); // ESC @ init
log::info!(
    "reconnect: Verification write result: {:?}",
    verify_result
);
```

### 5. Add Structured Log Fields for Monitoring

Use structured logging format for easier parsing by log aggregators:

```rust
// Example structured log format
log::info!(
    target: "print_ops",
    "{{\"event\":\"usb_write\",\"print_id\":\"{}\",\"bytes_expected\":{},\"bytes_written\":{},\"endpoint\":\"0x{:02X}\",\"duration_us\":{},\"success\":{}}}",
    print_id,
    data.len(),
    bytes_written,
    self.output_endpoint,
    write_start.elapsed().as_micros(),
    bytes_written == data.len()
);
```

### 6. Add Health Check Result Logging with Context

Enhance health check logging to include more diagnostic context:

```rust
// In check_connection()
log::info!(
    "check_connection: result={} | last_print_success={} | reconnect_count={} | uptime={:?}",
    healthy,
    last_print_ok,
    reconnect_counter,
    service_start.elapsed()
);
```

### 7. Summary Log at End of Each Print Operation

Add a single summary line that captures the key metrics for quick scanning:

```rust
log::info!(
    "[PRINT_SUMMARY] print_id={} | status={} | commands={} | bytes={}/{} | attempts={} | duration={:?}",
    print_id,
    if success { "OK" } else { "FAILED" },
    cmd_count,
    total_bytes_written,
    total_bytes_expected,
    attempt_count,
    total_elapsed
);
```

### Log Patterns to Alert On

Configure monitoring/alerting for these log patterns:

| Pattern | Severity | Action |
|---------|----------|--------|
| `USB partial write: wrote 0/` | Critical | Immediate alert |
| `ZERO_WRITE_ANOMALY` | Critical | Immediate alert |
| `bytes=0/` in PRINT_SUMMARY | Critical | Immediate alert |
| `USB write FAILED` repeated 3+ times | High | Alert after 3 occurrences |
| `reconnect:` more than 5 in 1 minute | High | Investigate USB stability |
| `success=false` in structured logs | Medium | Track failure rate |

### Example: Enhanced Log Output

**Before (current):**
```
[INFO] USB write OK: 0/2 bytes in 80.7µs
[WARN] USB partial write: expected 2 bytes, wrote 0 bytes
[INFO] execute_commands: COMPLETE - 4 commands in 1.3166ms
[INFO] with_retry: SUCCESS on attempt #1
```

**After (recommended):**
```
[INFO] [print_id=a1b2c3d4] Starting print operation with 4 commands
[ERROR] [print_id=a1b2c3d4] [PRINT_FAILURE] USB partial write: wrote 0/2 bytes | endpoint=0x01 | elapsed=80.7µs
[ERROR] [print_id=a1b2c3d4] [ZERO_WRITE_ANOMALY] Print reported success but 0 bytes transferred!
[INFO] [PRINT_SUMMARY] print_id=a1b2c3d4 | status=FAILED | commands=4 | bytes=0/30 | attempts=1 | duration=1.35ms
```

---

## References

- Debug Log (Failed): `reika-debug - Copy.log`
- Debug Log (Working): `reika-debug - Copy (2).log`
- Related Commit: `c0664df fix: improve USB connection stability and add watchdog for reliability`
