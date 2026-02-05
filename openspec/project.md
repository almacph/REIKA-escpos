# Project Context

## Purpose
REIKA-escpos is an ESC/POS thermal printer service for the REIKA POS system. It provides:
- A local HTTP API server for sending print commands to USB thermal printers
- A Windows GUI application with system tray integration for easy management
- Support for 40 ESC/POS commands including text formatting, barcodes, and 2D codes
- Automatic USB reconnection and error recovery with infinite retry

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

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                     REIKA-escpos                            │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────────┐ │
│  │   GUI App   │◄──►│  Server     │◄──►│  Consumer App   │ │
│  │  (egui)     │    │  (Warp)     │    │  (REIKA)        │ │
│  └──────┬──────┘    └──────┬──────┘    └─────────────────┘ │
│         │                  │                                │
│         │  watch::channel  │                                │
│         │  (status)        │                                │
│         ▼                  ▼                                │
│  ┌─────────────────────────────────────┐                   │
│  │         PrinterService              │                   │
│  │  - Infinite retry with reconnect    │                   │
│  │  - Print ID tracing                 │                   │
│  │  - Status broadcasting              │                   │
│  └──────────────────┬──────────────────┘                   │
│                     │                                       │
│                     ▼                                       │
│  ┌─────────────────────────────────────┐                   │
│  │       CustomUsbDriver               │                   │
│  │  - Partial write detection          │                   │
│  │  - Interface claim retry            │                   │
│  │  - Instrumented operations          │                   │
│  └──────────────────┬──────────────────┘                   │
│                     │                                       │
│                     ▼                                       │
│  ┌─────────────────────────────────────┐                   │
│  │      USB Thermal Printer            │                   │
│  │  (via WinUSB driver)                │                   │
│  └─────────────────────────────────────┘                   │
└─────────────────────────────────────────────────────────────┘
```

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
| **App** (`src/app/`) | GUI, tray, notifications, config, logging |

**Key Patterns:**
- `PrinterService` uses `Arc<Mutex<UsbDriver>>` for thread-safe USB access
- Automatic retry with reconnection on USB failures (infinite loop, 5s backoff)
- Command pattern for ESC/POS operations (`#[serde(tag = "command", content = "parameters")]`)
- Watch channels for status updates between server thread and GUI
- Atomic bools for lock-free tray event signaling

### Git Workflow
- Single `main` branch
- Conventional commit messages with type prefixes:
  - `fix:` for bug fixes
  - `feat:` for new features
  - `refactor:` for code restructuring
  - `docs:` for documentation changes
  - `ci:` for CI/CD changes
- Automatic releases on push to main via GitHub Actions

## Key Implementation Details

### Reliability Requirements
- **Infinite Retry**: Print operations never return error - they retry until success
- **Auto-Reconnect**: USB reconnection happens automatically after each failure
- **Partial Write Detection**: Treats incomplete USB writes as errors (prevents silent failures)
- **No Panics**: No `unwrap()` or `expect()` in production paths
- **Status Broadcasting**: GUI always knows current printer state via watch channel

### Print ID Tracing
Every print job gets unique 8-hex-char ID for RCA:
```
[print_id=a1b2c3d4] Starting print operation...
[print_id=a1b2c3d4] Attempt #1 FAILED after 2.5s: USB timeout
[print_id=a1b2c3d4] Reconnecting before retry...
[print_id=a1b2c3d4] Attempt #2 starting...
[PRINT_SUMMARY] print_id=a1b2c3d4 | status=OK | attempts=2 | duration=7.8s
```

### USB Driver Features
- Interface claim with 5 retry attempts (100ms delay)
- Endpoint auto-discovery or manual override
- 5-second operation timeout
- Clear halt on connect (recover from stale toggles)
- Detailed timing in logs (lock time, write time, total time)

### GUI Features
- Single instance enforcement via Windows named mutex
- System tray with dynamic green/red icon based on status
- Exit only via tray menu (window X minimizes)
- Receipt preview from logged commands
- Configurable printer presets (Standard, ICS Advent, Manual)

## Domain Context
**ESC/POS Protocol**: Standard command language for POS thermal printers. Commands are sent as byte sequences to control text formatting, feed paper, cut receipts, print barcodes/QR codes, and open cash drawers.

**USB Driver Setup**: Windows requires WinUSB driver (via Zadig) instead of default printer driver for direct USB communication.

**Key Hardware:**
- Default USB VID: `0x0483`
- Default USB PID: `0x5840`
- Default endpoint: Auto-discovered (typically 0x02)
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
- `notify-rust` (4) - Windows toast notifications
- `windows` (0.58) - Windows API bindings

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
    { "command": "Qrcode", "parameters": "https://receipt.example.com/123" },
    { "command": "PrintCut" }
  ]
}
```

## Specifications

See `openspec/specs/` for detailed capability specifications:

| Spec | Description |
|------|-------------|
| `usb-driver/spec.md` | USB communication with partial write detection |
| `printer-service/spec.md` | Retry/reconnect logic, print ID tracing |
| `escpos-commands/spec.md` | All 40 ESC/POS commands with byte mappings |
| `http-api/spec.md` | REST endpoints, handlers, CORS |
| `gui/spec.md` | System tray, window management, notifications |
| `configuration/spec.md` | Printer presets, server config, logging |

## Key Files

| Path | Description |
|------|-------------|
| `src/main.rs` | Entry point - GUI app with server thread |
| `src/services/printer.rs` | PrinterService with retry/reconnect logic |
| `src/services/usb_driver.rs` | Custom USB driver with partial write detection |
| `src/handlers/print.rs` | HTTP request handlers |
| `src/models/command.rs` | 40 ESC/POS command definitions |
| `src/app/gui.rs` | Main GUI application |
| `src/app/tray.rs` | System tray with Windows message pump |
| `src/app/config.rs` | Configuration loading and presets |
| `src/app/print_log.rs` | Persistent print job history |
| `PLAN/REIKA-NOTES.md` | Full API specification |
