## ADDED Requirements

### Requirement: REIKA Integration Configuration

The configuration file (`config.toml`) SHALL support a `[reika]` section with the following fields:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `api_key` | String | `""` (empty) | 64-character hex API key for REIKA sensor authentication |
| `server_url` | String | `"https://reika.local"` | REIKA server base URL |

Example `config.toml`:
```toml
[reika]
api_key = "a1b2c3d4e5f6..."
server_url = "https://reika.local"
```

When `api_key` is non-empty, the sensor reporter SHALL be started on application launch using the configured `server_url`. When `api_key` is empty, the sensor reporter SHALL not start.

The `AppConfig` struct SHALL include a `reika` field of type `ReikaConfig` with the above fields, deserialized from the TOML configuration.

#### Scenario: Load REIKA config with values
- **WHEN** the application starts and `config.toml` contains a `[reika]` section with valid `api_key` and `server_url`
- **THEN** the sensor reporter is started with the configured API key and server URL

#### Scenario: Load REIKA config without section
- **WHEN** the application starts and `config.toml` does not contain a `[reika]` section
- **THEN** the REIKA config defaults to empty `api_key` and `server_url` of `https://reika.local`, and the sensor reporter does not start because `api_key` is empty

#### Scenario: Save REIKA config from settings
- **WHEN** the operator enters an API key and server URL in the settings window and clicks Save
- **THEN** the `[reika]` section is written to `config.toml` with the entered values
