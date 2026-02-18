## ADDED Requirements

### Requirement: Reprint Button in Receipt Preview

The receipt preview window SHALL display a "Reprint" button below the receipt status information and above the receipt mockup.

The button SHALL only be enabled when:
- The preview entry has stored commands (`commands` field is `Some`)
- The printer is currently online

When clicked, the button SHALL send the stored commands through the reprint execution flow (with marker injection) without logging the operation to the print log.

While the reprint is in progress, the button SHALL be disabled and display "Reprinting..." to prevent duplicate submissions.

After completion, a Windows toast notification SHALL indicate success or failure.

#### Scenario: Reprint from preview window
- **WHEN** an operator views a historical receipt in the preview window and clicks "Reprint"
- **THEN** the stored commands are sent to the printer with reprint markers injected at top, middle, and bottom

#### Scenario: Reprint button disabled when offline
- **WHEN** the printer is offline and the operator views a receipt preview
- **THEN** the "Reprint" button is grayed out and not clickable

#### Scenario: Reprint button disabled for entries without commands
- **WHEN** the operator views a log entry that has no stored commands (e.g., a test print)
- **THEN** the "Reprint" button is not displayed

### Requirement: REIKA Sensor Settings

The settings window SHALL include a "REIKA Integration" section with the following fields:
- **API Key**: Text input for the 64-character hex sensor API key
- **Server URL**: Text input for the REIKA server base URL (defaults to `https://reika.local`)

These values SHALL be persisted to `config.toml` under a `[reika]` section and loaded on application startup.

The settings section SHALL display a brief description: "Connect to REIKA sensor dashboard for health monitoring".

When the API key is empty, the section SHALL display a note: "Leave empty to disable sensor reporting".

#### Scenario: Configure REIKA integration
- **WHEN** the operator enters an API key and server URL in the REIKA Integration settings and clicks Save
- **THEN** the values are persisted to `config.toml` under `[reika]` and a restart notice is shown

#### Scenario: Clear REIKA integration
- **WHEN** the operator clears the API key field and saves settings
- **THEN** the sensor reporter is disabled on next restart and no reports are sent
