## 1. Reprint Feature

### 1.1 Formatting State Tracker
- [x] 1.1.1 Create `FormattingState` struct in `src/services/reprint.rs` tracking: bold, underline, double_strike, reverse, justify, size, smoothing, flip, upside_down, font
- [x] 1.1.2 Implement `FormattingState::apply(command)` to update state from a command
- [x] 1.1.3 Implement `FormattingState::reset_commands()` returning commands to reset to defaults
- [x] 1.1.4 Implement `FormattingState::restore_commands()` returning commands to re-apply current state (only emit non-default values)

### 1.2 Reprint Marker Injection
- [x] 1.2.1 Implement `build_reprint_marker_commands(timestamp)` that generates the reversed marker block (separator, `** REPRINT COPY **`, timestamp, `REIKA-escpos`, separator)
- [x] 1.2.2 Implement `find_content_midpoint(commands)` that counts content-producing commands and returns the index to split at
- [x] 1.2.3 Implement `inject_reprint_markers(commands)` that:
  - Builds top marker + original first half
  - Scans state up to midpoint, injects mid marker with state save/restore
  - Appends second half + bottom marker before final cut

### 1.3 Reprint Execution
- [x] 1.3.1 Add `execute_reprint_commands(commands: Commands)` to `PrinterService` that calls `inject_reprint_markers` then delegates to `with_retry` / `execute_reprint_inner`
- [x] 1.3.2 Ensure reprint uses `with_retry` for reliability but does NOT call print log methods

### 1.4 Reprint HTTP Endpoint
- [x] 1.4.1 Add `handle_reprint` handler in `src/handlers/print.rs` — deserialize commands, call `service.execute_reprint_commands()`, return `StatusResponse`, send toast notification, skip print log
- [x] 1.4.2 Add `POST /print/reprint` route in `src/routes/print.rs`
- [x] 1.4.3 Register the new route in `print_routes()` combinator

### 1.5 GUI Reprint Button
- [x] 1.5.1 Add reprint state fields to `PrinterApp`: `reprint_in_progress: Arc<AtomicBool>`, communication channel (`std::sync::mpsc`) for triggering reprint from GUI thread to server thread
- [x] 1.5.2 Add "Reprint" button to `render_preview_window()` — enabled when commands exist and printer is online, disabled during reprint
- [x] 1.5.3 Wire button click to send stored commands through reprint flow (via mpsc channel)
- [x] 1.5.4 Handle reprint completion: re-enable button, show toast notification

## 2. Sensor Health Reporting

### 2.1 Configuration
- [x] 2.1.1 Add `ReikaConfig` struct to `src/app/config.rs` with `api_key: String` and `server_url: String` (default `https://reika.local`)
- [x] 2.1.2 Add `reika: ReikaConfig` field to `AppConfig` with serde default
- [x] 2.1.3 Add REIKA Integration section to settings window in `src/app/gui.rs` with API Key and Server URL text fields
- [x] 2.1.4 Wire save/cancel for the new settings fields

### 2.2 Sensor Reporter Module
- [x] 2.2.1 Add `reqwest` dependency to `Cargo.toml` with `json` and `rustls-tls` features, no default features
- [x] 2.2.2 Create `src/services/sensor_reporter.rs` with `SensorReporter` struct holding: reqwest client (insecure TLS), API key, server URL, current state
- [x] 2.2.3 Implement `SensorReporter::new(api_key, server_url)` constructing the HTTPS client with `danger_accept_invalid_certs(true)`
- [x] 2.2.4 Implement `SensorReporter::report(value: &str)` sending `POST {server_url}/api/sensors/report` with `X-Sensor-Key` header and `{ "value": "..." }` body
- [x] 2.2.5 Implement `SensorReporter::run(watch_rx, mpsc_rx)` as the main loop:
  - Every 60s: send heartbeat with current state
  - On watch change: send ONLINE/OFFLINE immediately
  - On mpsc event: send USB_ERROR/PRINT_FAIL immediately
  - On HTTP failure: log warning, continue
- [x] 2.2.6 Export module in `src/services/mod.rs`

### 2.3 Integration with Existing Services
- [x] 2.3.1 Create `SensorEvent` enum: `UsbError(String)`, `PrintFail(String)`
- [x] 2.3.2 Add `mpsc::Sender<SensorEvent>` to `PrinterService` (optional, only when sensor reporting is configured)
- [x] 2.3.3 Send `SensorEvent::PrintFail` from `with_retry` on command execution failure
- [x] 2.3.4 Send `SensorEvent::UsbError` from `CustomUsbDriver` on partial/zero writes and write failures
- [x] 2.3.5 Spawn `SensorReporter::run()` as background Tokio task in `src/main.rs` (conditionally, only when API key is configured)
- [x] 2.3.6 Pass the existing `watch::Receiver<bool>` (printer online status) to the sensor reporter

## 3. Build Verification
- [x] 3.1 Run `./build-windows.sh` and verify cross-compilation succeeds with new `reqwest` dependency
- [x] 3.2 Verify no `unwrap()`/`expect()` calls added in production paths
