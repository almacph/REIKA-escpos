## MODIFIED Requirements

### Requirement: Printer Status Panel

The GUI SHALL display current preset, VID/PID, configured endpoint details, USB transport state, physical readiness or unknown capability, busy/recovery state, and observation age. It SHALL distinguish setup/access errors from unplugged hardware and verified paper/cover faults, and SHALL NOT label unknown physical readiness as verified ready. Notifications SHALL occur only for meaningful state transitions.

#### Scenario: Standard preset display
- **WHEN** the Standard preset is selected
- **THEN** the panel shows Standard and USB 0x0483:0x5840 with current health details

#### Scenario: Manual preset display
- **WHEN** Manual uses a custom endpoint
- **THEN** the panel shows Manual, custom VID/PID, and endpoint

#### Scenario: Transport available but readiness unsupported
- **WHEN** the printer cannot provide verified status
- **THEN** the panel shows USB available and readiness unknown

#### Scenario: Known physical fault
- **WHEN** a verified profile reports paper-out or cover-open
- **THEN** the panel identifies that fault instead of suggesting only a USB cable check
