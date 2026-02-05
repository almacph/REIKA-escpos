# REIKA-escpos

ESC/POS thermal printer service for the REIKA POS system.

## Build Target

**Windows only.** This project is cross-compiled from Linux targeting Windows.

### Local Build & Test
```bash
./build-windows.sh
```
This script builds the release binary and creates a zip package at:
`target/x86_64-pc-windows-gnu/release/reika-escpos.zip`

### Manual Build
```bash
cargo build --release --target x86_64-pc-windows-gnu
```

Do not add Linux or macOS specific code. All platform-specific code should assume Windows 10/11.

## Reliability Requirements

This service runs on a **Windows 11 POS terminal** in a retail environment. Business continuity is critical:

### High Availability Design
- **Watchdog scripts** (`reika-watchdog.vbs`) monitor the service and restart it if accidentally closed
- **Single instance enforcement** prevents duplicate processes via Windows named mutex
- **Auto-reconnect on USB failure** - the service continuously retries USB connection every 5 seconds
- **Retry with reconnect** - print operations automatically retry on failure after reconnecting USB

### Recovery-First Mindset
When implementing changes, prioritize recovery over failure:
- Prefer infinite retry loops with backoff over hard failures
- Treat partial USB writes as errors (prevents silent print failures)
- Never block the HTTP API indefinitely - return status and let client retry
- Update GUI status in real-time so operators know printer state

### Logging Requirements
Logs must be **robust and context-rich** to enable RCA (Root Cause Analysis) by agents or developers reviewing production incidents.

**Required context in logs:**
- `print_id` - Unique identifier to trace a print job across retries
- `attempt` number - Which retry attempt is being made
- `duration` - How long operations take (for detecting slowdowns)
- `error details` - Full error message, not just "failed"

**Log levels:**
- `info!` - Print job start/complete, reconnection events, status changes
- `warn!` - Recoverable failures, retry attempts
- `error!` - Failed operations (with full context)
- `debug!` - Command-by-command execution, USB operations

**Example patterns:**
```rust
log::info!("[print_id={}] Starting print operation...", print_id);
log::error!("[print_id={}] Attempt #{} FAILED after {:?}: {:?}", print_id, attempt, elapsed, error);
log::info!("[PRINT_SUMMARY] print_id={} | status=OK | attempts={} | duration={:?}", print_id, attempt, total_time);
```

**Why this matters:**
- Agents performing RCA need to correlate events across log lines
- `print_id` allows tracing a single job through retries and reconnects
- Timing data helps identify USB instability vs printer hardware issues
- Summary lines (`[PRINT_SUMMARY]`) provide quick status for log scanning

### What NOT to Do
- Don't add `panic!()` or `unwrap()` on fallible operations in production paths
- Don't assume USB connection is stable - it can disconnect at any time
- Don't exit the application on recoverable errors
- Don't block the main thread - use async/await or spawn threads

## Project Context

Read `openspec/project.md` for full project context including:
- Tech stack and dependencies
- Architecture patterns
- Code conventions
- Consumer application (REIKA) integration details
- API specification overview

<!-- OPENSPEC:START -->
# OpenSpec Instructions

These instructions are for AI assistants working in this project.

Always open `@/openspec/AGENTS.md` when the request:
- Mentions planning or proposals (words like proposal, spec, change, plan)
- Introduces new capabilities, breaking changes, architecture shifts, or big performance/security work
- Sounds ambiguous and you need the authoritative spec before coding

Use `@/openspec/AGENTS.md` to learn:
- How to create and apply change proposals
- Spec format and conventions
- Project structure and guidelines

Keep this managed block so 'openspec update' can refresh the instructions.

<!-- OPENSPEC:END -->

## OpenSpec Directory

The `openspec/` directory contains all project specifications and context:

| File | Purpose |
|------|---------|
| `project.md` | Project overview, tech stack, conventions, consumer details |
| `AGENTS.md` | Instructions for creating and applying change proposals |
| `changes/` | Change proposals and implementation specs |

**Always update OpenSpec** when:
- Adding new features or APIs
- Changing architectural patterns
- Modifying the consumer integration contract
- Documenting decisions for future reference

## Key Files

| Path | Description |
|------|-------------|
| `src/main.rs` | Entry point - GUI app with server thread |
| `src/services/printer.rs` | PrinterService with retry/reconnect logic |
| `src/services/usb_driver.rs` | Custom USB driver implementation |
| `src/handlers/print.rs` | HTTP request handlers |
| `src/models/command.rs` | 35+ ESC/POS command definitions |
| `PLAN/REIKA-NOTES.md` | Full API specification |