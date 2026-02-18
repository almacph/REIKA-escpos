# REIKA Upstream Integration Guide: ESC/POS Printer Sensor & Reprint

Instructions for the REIKA web application to integrate with the new sensor health reporting and reprint features from `reika-escpos`.

## 1. Sensor Device Registration

The ESC/POS printer service registers itself as a sensor device using REIKA's existing sensor infrastructure (`POST /api/sensors/report`). Device registration is **manual** — an operator must create the device entry in REIKA before the service can report.

### 1.1 Registration Steps (Settings UI)

Navigate to `/settings/sensors` and create a new sensor device with these values:

| Field | Value |
|-------|-------|
| Name | `POS Printer` (or terminal-specific name like `POS Printer - Register 1`) |
| Type | `escpos` |
| Key | A unique API key string (e.g. UUID or random token). This must match the `api_key` configured in the printer service's `config.toml` under `[reika]`. |

The printer service operator will paste this same key into the REIKA-escpos settings window under "REIKA Integration > API Key".

### 1.2 API Contract

The printer service sends:

```
POST /api/sensors/report
Headers:
  X-Sensor-Key: <the api key from registration>
  Content-Type: application/json

Body:
{
  "value": "<state string>"
}
```

The REIKA backend should match `X-Sensor-Key` to the registered sensor device and update its state/last-seen timestamp accordingly. This follows the same pattern used by ESP8266/ESP32 firmware sensors.

## 2. Sensor State Values

The printer service reports exactly four state values. Each maps to a distinct operational condition.

| Value | Meaning | Trigger | Severity |
|-------|---------|---------|----------|
| `ONLINE` | Printer connected and healthy | USB device found, health check passes | Normal |
| `OFFLINE` | Printer disconnected | USB device not found or health check fails | Warning |
| `USB_ERROR` | USB communication failure | Partial/zero write, stale handle, bulk transfer error | Critical |
| `PRINT_FAIL` | Print operation failed | Command execution error after retry attempt | Critical |

### 2.1 Reporting Behavior

- **Heartbeat**: Every 60 seconds, the service reports its current state (typically `ONLINE`). If no heartbeat is received for >120 seconds, the device should be considered unreachable.
- **State changes**: `ONLINE` <-> `OFFLINE` transitions are reported immediately when detected.
- **Critical events**: `USB_ERROR` and `PRINT_FAIL` are reported immediately on occurrence. These are transient — the service will continue retrying and may return to `ONLINE` after a successful reconnect.
- **Startup**: The service starts in `OFFLINE` state and transitions to `ONLINE` once it successfully opens the USB device.

### 2.2 State Transition Diagram

```
                  USB found
  [OFFLINE] ─────────────────> [ONLINE]
     ^                            |
     |    USB lost                |
     +<───────────────────────────+
     |                            |
     |    Partial write /         |    Print cmd
     |    bulk transfer error     |    execution error
     |         v                  v
     |    [USB_ERROR]        [PRINT_FAIL]
     |         |                  |
     |         +── reconnect ─────+
     |                  |
     +<─────────────────+ (if USB lost)
                        |
              [ONLINE] <+ (if reconnect succeeds)
```

## 3. Dashboard Display Recommendations

### 3.1 State Indicators

| State | Color | Icon | Label | Description shown to operator |
|-------|-------|------|-------|-------------------------------|
| `ONLINE` | Green | Solid circle or checkmark | Online | Printer connected and ready |
| `OFFLINE` | Gray or Red | Empty circle or X | Offline | Printer not detected. Check USB cable. |
| `USB_ERROR` | Red | Warning triangle | USB Error | USB communication failure. Service is reconnecting. |
| `PRINT_FAIL` | Orange/Red | Warning triangle | Print Failed | Print operation failed. Service is retrying. |

### 3.2 Stale / Unreachable State

If the sensor's `last_seen` timestamp exceeds **120 seconds** (2x the 60-second heartbeat interval), display:

| Color | Icon | Label | Description |
|-------|------|-------|-------------|
| Dark gray | Question mark | Unreachable | No heartbeat received. The printer service may have crashed or the network is down. |

This is distinct from `OFFLINE` — offline means the service is running but can't find the printer. Unreachable means the service itself is not communicating.

### 3.3 Event History / Log View (Optional)

If the sensor dashboard supports event history, each state change report can be logged as a timeline entry. Useful events to surface:

- `ONLINE` after `OFFLINE` — "Printer reconnected"
- `OFFLINE` after `ONLINE` — "Printer disconnected"
- `USB_ERROR` — "USB communication failure (possible hardware issue)"
- `PRINT_FAIL` — "Print job failed"
- Heartbeat gap > 120s — "Service unreachable"

### 3.4 Alert / Notification Rules (Optional)

Suggested alert thresholds for operators:

| Condition | Alert Level | Message |
|-----------|-------------|---------|
| State = `OFFLINE` for > 5 minutes | Warning | POS Printer has been offline for 5+ minutes |
| State = `USB_ERROR` | Immediate | POS Printer USB communication failure |
| State = `PRINT_FAIL` (3+ in 10 min) | Warning | POS Printer experiencing repeated print failures |
| Unreachable > 5 minutes | Critical | POS Printer service is not responding |

## 4. Reprint API

The printer service exposes a reprint endpoint that the REIKA web app can call to reprint historical receipts. The service automatically injects anti-fraud markers (reversed white-on-black `** REPRINT COPY **` blocks) at the top, middle, and bottom of the receipt.

### 4.1 Endpoint

```
POST http://localhost:55000/print/reprint
Content-Type: application/json

{
  "commands": [
    { "command": "Writeln", "parameters": "Receipt line 1" },
    { "command": "Bold", "parameters": true },
    { "command": "Writeln", "parameters": "Total: $42.00" },
    { "command": "PrintCut", "parameters": null }
  ]
}
```

The request body uses the same `{ "commands": [...] }` format as `POST /print`. Send the original receipt commands — the service handles marker injection server-side.

### 4.2 Responses

**Success (200)**
```json
{
  "is_connected": true
}
```

**Bad Request (400)** — malformed command JSON
```json
{
  "is_connected": false,
  "error": "Invalid input: ..."
}
```

**Printer Error (500)** — USB/hardware failure during print
```json
{
  "is_connected": false,
  "error": "Printer error: ..."
}
```

### 4.3 Behavior Notes

- Reprints are **not** logged to the print log (they are not new transactions)
- The service retries with USB reconnect on failure (same reliability as normal prints)
- A toast notification is shown on the POS terminal when the reprint completes or fails
- Anti-fraud markers cannot be suppressed — every call to `/print/reprint` produces marked output

### 4.4 What the Markers Look Like

Three identical marker blocks are injected (top, middle, bottom of receipt):

```
================================
     ** REPRINT COPY **
  2026-02-17  14:30:00
  REIKA-escpos
================================
```

The markers are printed in **reversed mode** (white text on black background), making them physically difficult to remove or alter on thermal paper. The middle marker prevents edge-cutting fraud where someone trims the top/bottom to pass a reprint as an original.

## 5. Configuration Reference

The printer service's `config.toml` includes:

```toml
[reika]
api_key = ""                        # Must match the key registered in REIKA /settings/sensors
server_url = "https://reika.local"  # REIKA server base URL
```

When `api_key` is empty, sensor reporting is disabled entirely (no HTTP requests are made). The operator configures these values through the GUI settings window or by editing `config.toml` directly.

TLS certificate validation is disabled (matching the ESP8266/ESP32 firmware pattern) to support self-signed certificates on the REIKA server.
