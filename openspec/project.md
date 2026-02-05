# Project Context

## Purpose
REIKA-escpos is an ESC/POS thermal printer service for the REIKA POS system. It provides:
- A local HTTP API server for sending print commands to USB thermal printers
- A Windows GUI application with system tray integration for easy management
- Support for 35+ ESC/POS commands including text formatting, barcodes, and 2D codes
- Automatic USB reconnection and error recovery

## Tech Stack
- **Language**: Rust 2021 edition
- **Async Runtime**: Tokio (multi-threaded)
- **HTTP Server**: Warp
- **USB Communication**: rusb + escpos crate with custom USB driver
- **GUI Framework**: egui/eframe
- **System Tray**: tray-icon
- **Serialization**: serde + serde_json
- **Configuration**: toml
- **Logging**: log + env_logger + chrono
- **Notifications**: notify-rust (Windows toast)
- **Target Platform**: Windows (cross-compiled from Linux using MinGW)

## Project Conventions

### Code Style
- Standard Rust formatting (`cargo fmt`)
- Use `snake_case` for functions/variables, `PascalCase` for types/enums
- Prefer `Result<T, AppError>` for fallible operations
- Use descriptive log messages with context (print_id, attempt number, timing)
- Keep modules focused: one concern per file

### Architecture Patterns
**Layered Architecture:**
| Layer | Responsibility |
|-------|---------------|
| **Routes** (`src/routes/`) | Path definitions, HTTP methods, CORS |
| **Handlers** (`src/handlers/`) | Request/response transformation, calls services |
| **Services** (`src/services/`) | Business logic, USB device management, reconnection |
| **Models** (`src/models/`) | Data structures, serde serialization |
| **Error** (`src/error.rs`) | Error types → HTTP status codes |

**Key Patterns:**
- `PrinterService` uses `Arc<Mutex<UsbDriver>>` for thread-safe USB access
- Automatic retry with reconnection on USB failures
- Command pattern for ESC/POS operations (`#[serde(tag = "command", content = "parameters")]`)
- Watch channels for status updates between server thread and GUI

### Testing Strategy
- Manual testing with physical printers
- No automated test suite currently

### Git Workflow
- Single `main` branch
- Conventional commit messages with type prefixes:
  - `fix:` for bug fixes
  - `feat:` for new features
  - `refactor:` for code restructuring
  - `docs:` for documentation changes
  - `ci:` for CI/CD changes
- Automatic releases on push to main via GitHub Actions

## Domain Context
**ESC/POS Protocol**: Standard command language for POS thermal printers. Commands are sent as byte sequences to control text formatting, feed paper, cut receipts, print barcodes/QR codes, and open cash drawers.

**USB Driver Setup**: Windows requires WinUSB driver (via Zadig) instead of default printer driver for direct USB communication.

**Key Hardware:**
- Default USB VID: `0x0483`
- Default USB PID: `0x5840`
- Default endpoint: `0x02` (bulk out)
- API port: `55000`

## Important Constraints
- Windows-only deployment (cross-compiled from Linux)
- Single instance enforcement (only one copy can run)
- Requires WinUSB driver - printer won't work with standard Windows printing
- USB connection can be unstable - requires automatic retry/reconnection logic
- Partial USB writes must be treated as errors to prevent false success responses

## External Dependencies
**Crates:**
- `escpos` (0.17.0) - ESC/POS protocol implementation
- `rusb` (0.9.4) - USB device access
- `warp` (0.3.7) - HTTP server
- `eframe`/`egui` (0.29) - GUI framework
- `tray-icon` (0.19) - System tray integration

**Build Tools:**
- Rust 1.85+
- MinGW-w64 (for Windows cross-compilation)
- GitHub Actions for CI/CD

**Consumer Application - REIKA:**

| Field | Value |
|-------|-------|
| **Application** | REIKA (Real-time Efficient Inventory and Knowledge Administration) |
| **Repository** | https://github.com/almacph/REIKA |
| **Framework** | SvelteKit 5 (TypeScript) |
| **Schema Library** | Zod (TypeScript runtime validation) |
| **Integration** | Browser → HTTP → localhost:55000 |

**Key REIKA Source Files:**
- `src/lib/model/schemas/receiptSchema.ts` - Zod schema definitions for commands
- `src/lib/model/print.ts` - Printer status type definitions
- `src/routes/diagnose/PrinterTest.svelte` - Test UI component
- `src/routes/diagnose/+page.svelte` - Diagnostics page

## API Overview

**Endpoints:**
| Endpoint | Method | Description |
|----------|--------|-------------|
| `/print/test` | GET | Check printer connection status |
| `/print/test` | POST | Send test print line |
| `/print` | POST | Send ESC/POS commands |

**Response Schema:**
```json
{
  "is_connected": true,
  "error": "..." // Optional - only present on error
}
```

**Command Format:**
```json
{
  "commands": [
    { "command": "Init" },
    { "command": "Bold", "parameters": true },
    { "command": "Writeln", "parameters": "Hello World" },
    { "command": "PrintCut" }
  ]
}
```

See `PLAN/REIKA-NOTES.md` for full API specification with all 35+ commands.
