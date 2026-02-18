## ADDED Requirements

### Requirement: Reprint Endpoint

The system SHALL expose a `POST /print/reprint` endpoint that accepts the same JSON format as `POST /print`:

```json
{
  "commands": [
    { "command": "Init" },
    { "command": "Writeln", "parameters": "Hello" },
    { "command": "PrintCut" }
  ]
}
```

The endpoint SHALL inject reprint markers at top, middle, and bottom of the command stream and execute the modified commands on the printer.

The endpoint SHALL return the standard `StatusResponse` format:
- `200 OK` with `{ "is_connected": true }` on success
- `400 Bad Request` with error details for malformed input
- `500 Internal Server Error` for printer failures

The endpoint SHALL NOT log the reprint operation to the print log.

The endpoint SHALL trigger a Windows toast notification on completion (success or failure) with the prefix "Reprint" to distinguish from regular prints.

#### Scenario: Successful reprint via API
- **WHEN** a valid `POST /print/reprint` request is received with an array of ESC/POS commands
- **THEN** the system injects reprint markers, executes the commands on the printer, and returns `{ "is_connected": true }` with status 200

#### Scenario: Invalid reprint request
- **WHEN** a `POST /print/reprint` request is received with malformed JSON
- **THEN** the system returns status 400 with `{ "is_connected": false, "error": "Invalid input: ..." }`

#### Scenario: Reprint not logged
- **WHEN** a reprint is executed successfully
- **THEN** the print log file (`print_log.json`) is not modified and the GUI print log panel does not show a new entry
