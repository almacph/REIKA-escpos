## ADDED Requirements

### Requirement: Print Contract Preflight

POST /print and POST /print/reprint SHALL validate complete command payloads before any printer side effect. Malformed JSON, unknown command tags, and invalid command parameters SHALL return HTTP 400 with the existing `is_connected: false` and string `error` response fields. Request bodies larger than 4 MiB SHALL return HTTP 413 with those same response fields. Printer errors during a print attempt SHALL return to the requester without replaying the full command stream; transport recovery SHALL prepare the service for later requests.

#### Scenario: Valid consumer extensions
- **WHEN** a valid payload contains BitImage, CashDrawerPulse, Maxicode, or legacy boolean Italic
- **THEN** either endpoint accepts the command contract
- **AND** successful execution returns HTTP 200 with is_connected true

#### Scenario: Invalid late command
- **WHEN** a job starts with valid printable text but contains an invalid image later
- **THEN** HTTP 400 is returned before any job bytes are sent to USB

#### Scenario: Malformed JSON before handler dispatch
- **WHEN** a request body is syntactically invalid JSON
- **THEN** the rejection uses HTTP 400 and the standard JSON error fields

#### Scenario: Oversized body
- **WHEN** the request exceeds 4 MiB
- **THEN** HTTP 413 is returned with the standard JSON error fields and no USB output
