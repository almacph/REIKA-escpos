## MODIFIED Requirements

### Requirement: TOML Configuration File

The system SHALL load config.toml from the resolved state directory. Explicit portable mode SHALL use the executable directory; installed mode SHALL use the original interactive user's LOCALAPPDATA/REIKA-escpos directory. Existing ZIP deployments SHALL retain portable behavior until explicitly migrated. Missing configuration SHALL create defaults. Corrupt configuration SHALL be preserved, with a visible warning and recoverable defaults in memory. Saves SHALL use atomic replacement with a last-known-good copy and SHALL report failures without destroying existing data.

#### Scenario: Config file exists
- **WHEN** config.toml exists in the selected state directory
- **THEN** configuration is loaded from that file

#### Scenario: Config file missing
- **WHEN** config.toml does not exist
- **THEN** default configuration is created and saved to the selected writable state directory

#### Scenario: Config file corrupted
- **WHEN** config.toml has invalid TOML syntax
- **THEN** defaults are used in memory, the original is preserved, and recovery is offered

#### Scenario: Installed migration
- **WHEN** installed mode is first enabled and valid legacy configuration exists beside the executable
- **THEN** it is copied to the user's state directory only if the destination is absent
- **AND** the original is preserved and conflicts are reported without overwrite

### Requirement: Print Log Persistence

The system SHALL persist print history in print_log.json under the same resolved state directory as configuration. It SHALL add completed print entries newest-first, retain at most max_log_entries, and load existing entries on startup. Saves SHALL be atomic with recoverable prior data. Migration SHALL preserve legacy files and refuse to overwrite an existing destination. Persistence failure SHALL be visible and SHALL NOT cause an already delivered print to be resent.

#### Scenario: Log persistence
- **WHEN** a print job completes
- **THEN** an entry is added and saved to print_log.json

#### Scenario: Log trimming
- **WHEN** entries exceed max_log_entries
- **THEN** oldest entries are removed to stay within the limit

#### Scenario: Log loading
- **WHEN** the application starts
- **THEN** existing valid entries are loaded from the resolved state directory

#### Scenario: Interrupted write
- **WHEN** saving is interrupted or fails
- **THEN** the previous valid history remains recoverable and the failure is reported without resending the receipt

### Requirement: File Logging Configuration

The system SHALL enable or disable debug-file logging according to configuration. Enabled logging SHALL use reika-debug.log in the resolved state directory with DEBUG-level logging and bounded restart-safe retention. Disabled logging SHALL create no debug log and install a no-op logger.

#### Scenario: Logging enabled
- **WHEN** logging_enabled is true
- **THEN** the service appends to or rotates reika-debug.log in the resolved state directory

#### Scenario: Logging disabled
- **WHEN** logging_enabled is false
- **THEN** no debug log file is created and the no-op logger is used
