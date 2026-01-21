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
    command.rs         # Command enum (35+ ESC/POS commands) + type conversions
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
- `Command` - Tagged enum for 35+ ESC/POS commands (`#[serde(tag = "command", content = "parameters")]`)
- `StatusResponse` - API response with `is_connected: bool` and `error: Option<String>`
- `AppError` - `InvalidInput` (400), `PrinterError` (500), `Internal` (500)

## Requirements

- Rust 1.85+
- Windows: Zadig (for USB driver setup)

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

## Windows USB Driver Setup

The printer must use **WinUSB** driver (not the default Windows USB Print driver).

1. Download [Zadig](https://zadig.akeo.ie/)
2. Run as Administrator
3. **Options → List All Devices**
4. Select your printer (VID_0483 PID_5840)
5. Select **WinUSB** as target driver
6. Click **Replace Driver**

> Note: This makes the printer unavailable to standard Windows printing. Use Device Manager to restore if needed.

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

## Supported Commands

| Command | Parameters | Description |
|---------|------------|-------------|
| Print, Init, Reset, Cut, PartialCut, PrintCut, ResetSize, ResetLineSpacing | none | Basic operations |
| Bold, DoubleStrike, Flip, Reverse, Smoothing, Feed, UpsideDown | `bool` | Toggle features |
| Feeds, LineSpacing | `number` | Numeric settings |
| Write, Writeln | `string` | Text output |
| Size | `[width, height]` | Text size (1-8) |
| Justify | `LEFT`, `CENTER`, `RIGHT` | Alignment |
| Underline | `None`, `Single`, `Double` | Underline mode |
| Font | `A`, `B`, `C` | Font selection |
| CashDrawer | `Pin2`, `Pin5` | Open drawer |
| Ean13, Ean8, Upca, Upce, Code39, Codabar, Itf | `string` | Barcodes |
| Qrcode, Pdf417, DataMatrix, Aztec, MaxiCode, GS1Databar2d | `string` | 2D codes |
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
