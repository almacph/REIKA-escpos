## MODIFIED Requirements

### Requirement: Endpoint Auto-Discovery

The driver SHALL select a valid bulk OUT endpoint from one supported interface and alternate setting. Bulk IN SHALL be optional and discovered independently from actual descriptors. Manual interface/OUT selections SHALL be validated against those descriptors; the driver SHALL NOT derive IN by setting the high bit of OUT. Non-default alternate settings SHALL be activated before I/O. Ambiguous device/interface candidates SHALL require explicit selection or a validated profile rather than silent first-match switching.

#### Scenario: Auto-discovery finds bulk endpoints
- **WHEN** the selected interface exposes bulk OUT and bulk IN
- **THEN** the actual descriptor addresses are used even if they have different endpoint numbers

#### Scenario: Manual endpoint override
- **WHEN** endpoint and interface are configured
- **THEN** the OUT endpoint is validated and optional IN is discovered independently

#### Scenario: Output-only printer
- **WHEN** the interface has valid bulk OUT but no bulk IN
- **THEN** printing remains supported and physical readiness is unknown

#### Scenario: Invalid manual endpoint
- **WHEN** the configured endpoint is missing or has the wrong direction/type
- **THEN** the driver reports configuration failure instead of attempting I/O on an invented endpoint

## ADDED Requirements

### Requirement: Typed USB Failure and Connection Ownership

The driver SHALL preserve native error categories for recovery decisions and expose all handles only through the single USB owner. Invalid handles SHALL be released before reopen. Native calls SHALL have finite timeouts; a caller-side timeout SHALL NOT spawn overlapping native operations. The existing print transfer timeout SHALL remain 5 seconds; health transactions SHALL use their shorter explicit budget.

#### Scenario: Busy or access failure
- **WHEN** claim/open reports Busy or Access
- **THEN** diagnostics preserves that category and provides access/driver guidance

#### Scenario: Partial transfer
- **WHEN** a write completes partially or with zero bytes
- **THEN** it remains a failed transfer with uncertain delivery and is never reported as success
