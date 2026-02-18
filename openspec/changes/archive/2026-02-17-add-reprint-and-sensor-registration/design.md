## Context

Two new cross-cutting features are added to the REIKA ESC/POS service:

1. **Reprint**: Operators and the REIKA web app can reprint historical receipts with anti-fraud markers injected at three positions. The challenge is safely injecting formatted text into the middle of an ESC/POS command stream without corrupting stateful formatting commands.

2. **Sensor Health Reporting**: The service registers with REIKA's existing sensor monitoring infrastructure (`POST /api/sensors/report`) to report printer health states. Critical failures like USB partial writes (ref: RCA-2026-02-05-001) are reported immediately as state changes so they appear on the REIKA sensor dashboard.

### Stakeholders
- POS operators (reprint receipts, monitor service health on dashboard)
- REIKA web app (trigger reprints via API, view sensor health)
- REIKA sensor dashboard (receive health events from this service)

## Goals / Non-Goals

**Goals:**
- Safe mid-stream marker injection that preserves ESC/POS formatting state
- Anti-fraud protection: markers at top, middle, and bottom prevent edge-cutting
- No logging of reprints (not a new transaction)
- Real-time health reporting to REIKA sensor dashboard
- Critical failure events (USB errors, print failures) reported immediately
- Heartbeat reporting at a configurable interval

**Non-Goals:**
- Encryption of the API key in config.toml (plain text is acceptable per user requirement)
- Modifying the REIKA sensor dashboard UI (it already supports arbitrary device types)
- Registering the device programmatically (device registration is done manually in REIKA `/settings/sensors`)

## Decisions

### 1. Reprint Marker Injection: State-Save/Restore Pattern

**Decision:** Track ESC/POS formatting state by scanning commands up to the injection point, emit reset commands, inject the marker, then emit restore commands to re-apply the tracked state.

**Rationale:** ESC/POS formatting is stateful — commands like `Bold(true)`, `Size((2,3))`, `Justify(CENTER)` persist until explicitly changed. A naive injection would either inherit active formatting on the marker or lose it for subsequent commands.

**State fields to track:**
| Field | Default | Reset Command | Restore Command |
|-------|---------|---------------|-----------------|
| bold | false | `Bold(false)` | `Bold(true)` |
| underline | None | `Underline(None)` | `Underline(mode)` |
| double_strike | false | `DoubleStrike(false)` | `DoubleStrike(true)` |
| reverse | false | `Reverse(false)` | `Reverse(true)` |
| justify | LEFT | `Justify(LEFT)` | `Justify(mode)` |
| size | (1,1) | `ResetSize` | `Size((w,h))` |
| smoothing | false | `Smoothing(false)` | `Smoothing(true)` |
| flip | false | `Flip(false)` | `Flip(true)` |
| upside_down | false | `UpsideDown(false)` | `UpsideDown(true)` |
| font | A | `Font(A)` | `Font(f)` |

**Algorithm:**
```
1. Find midpoint: count content-producing commands (Write, Writeln, barcodes, QR codes)
   and identify the index at floor(count / 2)
2. Split original commands at the midpoint index
3. Build modified command list:
   a. [Init] + [marker_top_commands] + [restore_to_default_state]
   b. [first_half_of_original_commands]
   c. [save_state] + [reset_to_default] + [marker_mid_commands] + [restore_saved_state]
   d. [second_half_of_original_commands]
   e. [reset_to_default] + [marker_bottom_commands] + [PrintCut]
```

**Alternatives considered:**
- *Inject only at top/bottom*: Rejected — user specifically requires middle marker to prevent edge-cutting fraud.
- *Use `Init` at injection point*: Simpler but destroys all state including page code and character set, potentially breaking non-ASCII receipts.
- *Parse receipt structure (items vs totals)*: Too fragile — depends on receipt layout conventions that vary.

### 2. Marker Visual Style: Reversed (White-on-Black) Text

**Decision:** Use `Reverse(true)` for the reprint marker lines to make them visually distinct and physically harder to alter on thermal paper.

**Marker content:**
```
================================
     ** REPRINT COPY **
  2026-02-17  14:30:00
  REIKA-escpos
================================
```

**Rationale:** Reversed thermal printing (white text on black background) uses significantly more ink/heat. It cannot be cleanly scraped off or covered without visible damage, making it a practical anti-tamper measure for thermal receipts.

### 3. Midpoint Calculation: Content-Command Counting

**Decision:** The midpoint is calculated by counting content-producing commands (Write, Writeln, Ean13, Qrcode, etc.) and splitting at `floor(count / 2)`. The split happens *between* commands at a content boundary, never between a formatting command and its corresponding content command.

**Content-producing commands:**
- `Write`, `Writeln`
- All barcode commands: `Ean13`, `Ean8`, `Upca`, `Upce`, `Code39`, `Codabar`, `Itf`
- All 2D code commands: `Qrcode`, `GS1Databar2d`, `Pdf417`, `MaxiCode`, `DataMatrix`, `Aztec`

**Non-content commands (formatting only, never split here):**
- `Bold`, `Underline`, `Size`, `Justify`, `Reverse`, `Font`, `Flip`, etc.
- `Init`, `Reset`, `Feed`, `Feeds`, `LineSpacing`, `Cut`, etc.

### 4. Sensor Health Reporting: State-Change + Heartbeat Model

**Decision:** Follow the same pattern as the ESP8266 firmware (solenoid-http.ino) — report current state periodically as heartbeat, and report state changes immediately.

**State values:**
| Value | Trigger | Meaning |
|-------|---------|---------|
| `ONLINE` | Printer connected, health check passes | Normal operation |
| `OFFLINE` | Printer disconnected, USB not found | Connection lost |
| `USB_ERROR` | Partial/zero write, stale handle, interface claim failure | RCA-class USB failure |
| `PRINT_FAIL` | Print command execution failure after retry | Print operation failed |

**Heartbeat interval:** 60 seconds (matches KTV room monitor pattern). This is appropriate because:
- The service is on local WiFi/LAN, not battery-constrained like sensors
- 60s gives the dashboard near-real-time visibility
- State changes (errors) are reported immediately regardless of heartbeat timer

**Alternatives considered:**
- *5-minute heartbeat (solenoid pattern)*: Too slow for a POS service where operators need immediate visibility.
- *10-second heartbeat*: Unnecessarily aggressive for a status that changes infrequently.

### 5. Sensor Reporter Architecture: Background Tokio Task with Watch Channel

**Decision:** The sensor reporter runs as a background `tokio::spawn` task. It subscribes to the existing `watch::channel<bool>` for online/offline status, and receives critical error events via a new `tokio::sync::mpsc` channel from the printer service and USB driver.

**Architecture:**
```
PrinterService --[watch<bool>]--> SensorReporter --[HTTP POST]--> REIKA /api/sensors/report
     |                                  ^
     +--[mpsc<SensorEvent>]-------------+
     |
UsbDriver --[mpsc<SensorEvent>]--------+
```

**Why mpsc for errors:** The watch channel only tracks boolean online/offline. Critical error events (USB_ERROR, PRINT_FAIL) need to be reported as distinct state changes with immediate delivery. An mpsc channel allows the printer service and USB driver to fire-and-forget error events without blocking their main operations.

**Recovery:** If the REIKA server is unreachable, the reporter logs a warning and retries on the next heartbeat cycle. It does not block or crash — the service's primary function (printing) is never affected by sensor reporting failures.

### 6. HTTPS Client: reqwest with rustls-tls, Insecure Mode

**Decision:** Use `reqwest` with `rustls-tls` feature for the HTTPS client. TLS certificate verification is **disabled** (`.danger_accept_invalid_certs(true)`), matching the ESP8266 firmware pattern where `wifiClient.setInsecure()` skips cert validation for self-signed certificates on the REIKA server.

**Rationale:** The REIKA server uses a self-signed or internally-issued certificate. All sensor devices (ESP8266/ESP32) already connect in insecure mode. The POS service operates on the same local network and should follow the same pattern.

**Cargo.toml addition:**
```toml
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
```

**Client construction:**
```rust
reqwest::Client::builder()
    .danger_accept_invalid_certs(true)
    .timeout(Duration::from_secs(10))
    .build()
```

### 7. Reprint API: Accept Full Command Array

**Decision:** The `POST /print/reprint` endpoint accepts the same `{ "commands": [...] }` format as `POST /print`. The server-side injects the reprint markers. This keeps the REIKA frontend simple — it just sends the stored commands without needing to know about marker injection.

**Why not an index into the print log:** The HTTP API serves the REIKA web app which manages its own receipt history. The GUI reprint button uses the local print log index internally, but the API is command-based for flexibility.

## Risks / Trade-offs

| Risk | Mitigation |
|------|------------|
| Marker injection corrupts formatting | State tracking covers all 10 formatting properties; `Init`/`Reset` commands in the stream reset the tracked state |
| Midpoint lands in an awkward position (e.g., between a barcode and its label) | Split only at content-command boundaries, never between formatting + content pairs |
| reqwest adds binary size | Use minimal features (json + rustls-tls, no default features); acceptable trade-off for sensor reporting |
| REIKA server unreachable | Reporter logs warning and continues; never blocks printing operations |
| API key exposed in config.toml | Acceptable per user requirement; config.toml is local to the POS terminal |

## Open Questions

- None — all requirements have been clarified.
