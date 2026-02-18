# Change: Add receipt reprinting with anti-fraud markers and REIKA sensor health reporting

## Why
Operators need to reprint historical receipts from the GUI preview window, but reprints must be clearly marked at the top, middle, and bottom to prevent bad actors from cutting edges and passing them off as originals. Additionally, the ESC/POS service should register itself with REIKA's sensor monitoring infrastructure so operators can monitor service health and critical failures (like the false-success USB issue from RCA-2026-02-05-001) from the REIKA dashboard in real-time.

## What Changes

### Reprint Feature
- Add reprint capability to the receipt preview window with a "Reprint" button
- Inject reprint indicator lines at **top, middle, and bottom** of reprinted receipts
- Implement ESC/POS formatting state tracking to safely inject markers mid-stream without corrupting formatting context (bold, size, justify, underline, reverse, etc.)
- Reprints are **NOT** logged to the print log (not a new print job)
- Add `POST /print/reprint` HTTP endpoint so the REIKA web app can also trigger reprints

### Sensor Health Reporting
- Register the service as a sensor device with REIKA's `POST /api/sensors/report` endpoint
- Report state values reflecting service health:
  - `ONLINE` — Printer connected and healthy (heartbeat)
  - `OFFLINE` — Printer disconnected / USB not found
  - `PRINT_FAIL` — Print operation failed (retry exhaustion or critical error)
  - `USB_ERROR` — USB partial write, stale handle, or communication failure (RCA-class events)
- Heartbeat interval reporting (similar to ESP8266 firmware pattern)
- Report critical failures **immediately** on occurrence (state change triggers instant report)
- Add API key and REIKA server URL fields to the settings page and `config.toml`

## Impact
- Affected specs: `printer-service`, `http-api`, `gui`, `configuration`
- Affected code:
  - `src/services/printer.rs` — New `execute_reprint_commands` with state-save/restore injection
  - `src/services/sensor_reporter.rs` — New module: heartbeat loop + event reporting to REIKA
  - `src/handlers/print.rs` — New `handle_reprint` handler
  - `src/routes/print.rs` — New `/print/reprint` route
  - `src/app/gui.rs` — Reprint button in preview window, API key / server URL settings fields
  - `src/app/print_log.rs` — Index-based entry access for reprint
  - `src/app/config.rs` — New `reika` config section (api_key, server_url)
  - `src/models/request.rs` — `ReprintRequest` schema
  - `src/services/usb_driver.rs` — Hook sensor reporter on critical USB failures
  - `Cargo.toml` — Add `reqwest` dependency for HTTP client
