## MODIFIED Requirements

### Requirement: Status Check Endpoint (GET /print/test)

GET /print/test SHALL return HTTP 200 with the existing is_connected boolean and optional error field from the cached health snapshot without issuing USB I/O. The boolean SHALL be true for fresh available transport with no known blocking physical fault, including unsupported physical readiness, and false for unavailable or stale transport or a known blocking fault. It SHALL NOT claim physical completion or paper readiness solely from a successful USB write.

#### Scenario: Printer connected
- **WHEN** fresh transport is available with no known blocking fault
- **THEN** GET /print/test returns 200 with is_connected true

#### Scenario: Printer disconnected
- **WHEN** the printer transport is unavailable
- **THEN** GET /print/test returns 200 with is_connected false and a descriptive error

#### Scenario: USB operation blocked
- **WHEN** a native USB operation is blocked for 5 seconds in the test backend
- **THEN** GET /print/test answers from cache within the 250 ms test budget

## ADDED Requirements

### Requirement: Detailed Printer Status Endpoint

GET /printer/status SHALL return a cached JSON snapshot containing transport, readiness, busy, status_capability, reason, connection generation, observation timestamps, and staleness. It SHALL perform no USB I/O and SHALL remain responsive during discovery, printing, and recovery. Service GET /health SHALL remain the independent watchdog liveness probe.

#### Scenario: Output-only printer
- **WHEN** transport is available without verified status capability
- **THEN** the detailed response distinguishes available transport from unknown readiness

#### Scenario: Blocked native call
- **WHEN** the test backend blocks a USB write for 5 seconds
- **THEN** the detailed endpoint responds within 250 ms from cache

### Requirement: Bounded Print Admission

Print and API reprint requests exceeding the 32-request pending capacity SHALL receive HTTP 503 with is_connected false and a descriptive error before any USB output. Accepted requests SHALL retain existing success response semantics.

#### Scenario: Pending queue full
- **WHEN** another print is submitted with no pending slot available
- **THEN** it is rejected before transmission without disturbing accepted jobs
