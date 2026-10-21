# REIKA-escpos

ESC/POS thermal printer service for the REIKA POS system.

## Project Structure

```
src/
  main.rs              # Entry point - initializes PrinterService and starts server
  server.rs            # Warp server startup (8 lines)
  error.rs             # AppError enum with HTTP status code mapping

  routes/
    mod.rs             # Route composition + CORS configuration
    print.rs           # Route definitions for /print endpoints

  handlers/
    mod.rs             # Handler exports
    print.rs           # Request handlers (status, test print, print commands)

  services/
    mod.rs             # Service exports
    printer.rs         # PrinterService - USB driver management, command execution

  models/
    mod.rs             # Model re-exports
    command.rs         # Command enum (42 ESC/POS commands) + type conversions
    request.rs         # Request DTOs (Commands, PrinterTestSchema)
    response.rs        # StatusResponse with optional error field
```

### Architecture

| Layer | Responsibility |
|-------|---------------|
| **Routes** | Path definitions, HTTP methods, CORS |
| **Handlers** | Request/response transformation, calls services |
| **Services** | Business logic, USB device management, reconnection |
| **Models** | Data structures, serde serialization |
| **Error** | Error types → HTTP status codes |

### Key Types

- `PrinterService` - Thread-safe wrapper (`Arc<Mutex<UsbDriver>>`) with auto-reconnect
- `Command` - Tagged enum for 42 ESC/POS commands (`#[serde(tag = "command", content = "parameters")]`)
- `StatusResponse` - API response with `is_connected: bool` and `error: Option<String>`
- `AppError` - `InvalidInput` (400), `PrinterError` (500), `Internal` (500)

## Requirements

- Rust 1.85+
- Windows 10/11 x64: signed REIKA Setup EXE (development ZIP remains available)

## Building

```bash
# Linux (native)
cargo build --release

# Windows (cross-compile from Linux)
rustup target add x86_64-pc-windows-gnu
sudo apt install mingw-w64
cargo build --release --target x86_64-pc-windows-gnu
```

Output: `target/release/reika-escpos` or `target/x86_64-pc-windows-gnu/release/reika-escpos.exe`

## Windows Setup and USB Driver

Production installs use the signed `reika-setup.exe`, which requests elevation
for the reviewed driver package and registers a per-user logon watchdog. The
installer replaces the Windows USB Print binding for the selected interface;
that printer will no longer be available through the standard spooler. Repair
and uninstall remove the task and application files while leaving shared
driver-store packages unless Windows confirms safe removal.

The installer refuses to claim plug-and-play setup when the signed INF/CAT
package, trusted signing provenance, or captured composite interface data is
missing. The portable ZIP is suitable for development and recovery. In that
case, use [Zadig](https://zadig.akeo.ie/) as a clearly labeled fallback:
run it as Administrator, enable **Options → List All Devices**, select the
reviewed printer interface, and choose **WinUSB**, **NOT libusbK or
libusb-win32 (libusb0)**. This fallback is not the
production installer path; restoring USBPRINT may be required for spooler use.

The watchdog checks the exact installed executable and bounded `GET /health`
responses. Three consecutive liveness failures trigger a restart after a
five-minute cooldown. USB unplug/readiness failures do not restart a healthy
service. Driver repair can create a short-lived `maintenance.lease` in the original user state directory rather than beside the
installed executable; the watchdog ignores checks for at most 15 minutes.

## Configuration

| Setting | Value |
|---------|-------|
| Port | `55000` |
| Host | `127.0.0.1` |
| USB VID | `0x0483` |
| USB PID | `0x5840` |

## API Endpoints

All endpoints return JSON `StatusResponse`:

```typescript
{ is_connected: boolean, error?: string }
```

### GET /print/test
Check printer connection status.

```json
{"is_connected": true}
```

### POST /print/test
Send test print.

```json
{"test_line": "Hello World", "test_page": false}
```

### POST /print
Send ESC/POS commands.

```json
{
  "commands": [
    {"command": "Init"},
    {"command": "Bold", "parameters": true},
    {"command": "Writeln", "parameters": "Hello World"},
    {"command": "PrintCut"}
  ]
}
```

### POST /print/reprint
Reprint commands with the existing anti-fraud markers. Reprints are not added to print history.

## Supported Commands

| Command | Parameters | Description |
|---------|------------|-------------|
| Print, Init, Reset, Cut, PartialCut, PrintCut, ResetSize, ResetLineSpacing | none | Basic operations |
| Bold, Italic, DoubleStrike, Flip, Reverse, Smoothing, Feed, UpsideDown | `bool` | Toggle features |
| Feeds, LineSpacing | `number` | Numeric settings |
| Write, Writeln | `string` | Text output |
| Size | `[width, height]` | Text size (1-8) |
| Justify | `LEFT`, `CENTER`, `RIGHT` | Alignment |
| Underline | `None`, `Single`, `Double` | Underline mode |
| Font | `A`, `B`, `C` | Font selection |
| CashDrawer | `Pin2`, `Pin5` | Open drawer with legacy timing |
| CashDrawerPulse | `{pin, pulseMs}` (`100`, `150`, `200`) | Open drawer with explicit pulse timing |
| Ean13, Ean8, Upca, Upce, Code39, Codabar, Itf | `string` | Barcodes |
| Qrcode, Pdf417, DataMatrix, Aztec, MaxiCode/Maxicode, GS1Databar2d | `string` | 2D codes |
| BitImage | `{width, height, data}` | 1-bit base64 raster; width 1–576, height 1–4096, max 294,912 decoded bytes |
| PageCode, CharacterSet | enum values | Character encoding |

## Integration

```javascript
// Check status
const status = await fetch('http://localhost:55000/print/test').then(r => r.json());

// Print receipt
await fetch('http://localhost:55000/print', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ commands: [...] })
});
```

## Related Documentation

- [REIKA-NOTES.md](REIKA-NOTES.md) - Full API specification with examples and consumer integration notes

## License

MIT
