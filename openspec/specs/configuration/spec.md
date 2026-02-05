# Configuration Specification

Application configuration system using TOML format with printer presets.

## Requirements

### Requirement: TOML Configuration File

The system SHALL load configuration from `config.toml` in the executable directory.

**Implementation:** `src/app/config.rs:70-95`

```rust
const CONFIG_FILENAME: &str = "config.toml";

impl AppConfig {
    pub fn config_path() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
            .join(CONFIG_FILENAME)
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(contents) = fs::read_to_string(&path) {
                if let Ok(config) = toml::from_str(&contents) {
                    return config;
                }
            }
        }

        // Create default config if not found
        let config = Self::default();
        let _ = config.save();
        config
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::config_path();
        let contents = toml::to_string_pretty(self)?;
        fs::write(path, contents)?;
        Ok(())
    }
}
```

#### Scenario: Config file exists
- **WHEN** `config.toml` exists in executable directory
- **THEN** configuration is loaded from file

#### Scenario: Config file missing
- **WHEN** `config.toml` does not exist
- **THEN** default configuration is created and saved

#### Scenario: Config file corrupted
- **WHEN** `config.toml` has invalid TOML syntax
- **THEN** default configuration is used

---

### Requirement: Configuration Structure

The system SHALL organize configuration into printer, server, and UI sections.

**Implementation:** `src/app/config.rs:10-50`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub printer: PrinterConfig,
    pub server: ServerConfig,
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterConfig {
    #[serde(default)]
    pub preset: PrinterPreset,
    #[serde(default)]
    pub vendor_id: Option<u16>,
    #[serde(default)]
    pub product_id: Option<u16>,
    #[serde(default)]
    pub endpoint: Option<u8>,
    #[serde(default)]
    pub interface: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub max_log_entries: usize,
    #[serde(default)]
    pub logging_enabled: bool,
}
```

#### Scenario: Complete config file
```toml
[printer]
preset = "standard"

[server]
port = 55000

[ui]
max_log_entries = 100
logging_enabled = false
```

#### Scenario: Manual preset config
```toml
[printer]
preset = "manual"
vendor_id = 1155
product_id = 22592
endpoint = 2
interface = 0

[server]
port = 55000

[ui]
max_log_entries = 100
logging_enabled = true
```

---

### Requirement: Printer Presets

The system SHALL support three printer presets with predefined USB parameters.

**Implementation:** `src/app/config.rs:52-110`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PrinterPreset {
    #[default]
    Standard,
    IcsAdvent,
    Manual,
}

impl PrinterConfig {
    pub fn resolved_vendor_id(&self) -> u16 {
        match self.preset {
            PrinterPreset::Standard => 0x0483,
            PrinterPreset::IcsAdvent => 0x0FE6,
            PrinterPreset::Manual => self.vendor_id.unwrap_or(0x0483),
        }
    }

    pub fn resolved_product_id(&self) -> u16 {
        match self.preset {
            PrinterPreset::Standard => 0x5840,
            PrinterPreset::IcsAdvent => 0x811E,
            PrinterPreset::Manual => self.product_id.unwrap_or(0x5840),
        }
    }

    pub fn resolved_endpoint(&self) -> Option<u8> {
        match self.preset {
            PrinterPreset::Standard => None,           // Auto-discover
            PrinterPreset::IcsAdvent => Some(1),       // Fixed endpoint
            PrinterPreset::Manual => self.endpoint,
        }
    }

    pub fn resolved_interface(&self) -> Option<u8> {
        match self.preset {
            PrinterPreset::Standard => None,           // Auto-discover
            PrinterPreset::IcsAdvent => Some(0),       // Fixed interface
            PrinterPreset::Manual => self.interface,
        }
    }
}
```

#### Scenario: Standard preset
- **WHEN** `preset = "standard"`
- **THEN** uses VID=0x0483, PID=0x5840, auto-discover endpoint/interface

#### Scenario: ICS Advent preset
- **WHEN** `preset = "ics_advent"`
- **THEN** uses VID=0x0FE6, PID=0x811E, endpoint=1, interface=0

#### Scenario: Manual preset
- **WHEN** `preset = "manual"`
- **THEN** uses values from `vendor_id`, `product_id`, `endpoint`, `interface` fields

---

### Requirement: Default Configuration

The system SHALL provide sensible defaults for all configuration values.

**Implementation:** `src/app/config.rs:112-135`

```rust
impl Default for AppConfig {
    fn default() -> Self {
        Self {
            printer: PrinterConfig {
                preset: PrinterPreset::Standard,
                vendor_id: None,
                product_id: None,
                endpoint: None,
                interface: None,
            },
            server: ServerConfig { port: 55000 },
            ui: UiConfig {
                max_log_entries: 100,
                logging_enabled: false,
            },
        }
    }
}
```

#### Scenario: Default values
- **WHEN** no config.toml exists
- **THEN** uses: Standard preset, port 55000, 100 log entries, logging disabled

---

### Requirement: USB Configuration Resolution

The system SHALL resolve USB parameters from preset or manual override.

**Implementation:** `src/main.rs:54-62`

```rust
let usb_config = UsbConfig {
    vendor_id: server_config.printer.resolved_vendor_id(),
    product_id: server_config.printer.resolved_product_id(),
    endpoint: server_config.printer.resolved_endpoint(),
    interface: server_config.printer.resolved_interface(),
};
```

#### Scenario: Preset resolution
- **WHEN** printer service starts
- **THEN** calls `resolved_*()` methods to get final USB parameters

#### Scenario: Manual fallback
- **WHEN** Manual preset with missing vendor_id
- **THEN** falls back to Standard's VID (0x0483)

---

### Requirement: File Logging Configuration

The system SHALL enable/disable file logging based on configuration.

**Implementation:** `src/main.rs:37-44`

```rust
if config.ui.logging_enabled {
    if let Err(e) = init_file_logging() {
        eprintln!("Failed to initialize file logging: {}", e);
    }
} else {
    init_noop_logging();
}
```

#### Scenario: Logging enabled
- **WHEN** `logging_enabled = true`
- **THEN** creates `reika-debug.log` with DEBUG level logging

#### Scenario: Logging disabled
- **WHEN** `logging_enabled = false`
- **THEN** no log file created, no-op logger installed

---

### Requirement: Print Log Persistence

The system SHALL persist print log to JSON file.

**Implementation:** `src/app/print_log.rs`

```rust
const LOG_FILENAME: &str = "print_log.json";

impl PrintLog {
    pub fn log_path() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
            .join(LOG_FILENAME)
    }

    pub fn load(max_entries: usize) -> Self {
        let path = Self::log_path();
        if path.exists() {
            if let Ok(contents) = fs::read_to_string(&path) {
                if let Ok(mut log) = serde_json::from_str::<Self>(&contents) {
                    log.max_entries = max_entries;
                    // Trim to max_entries
                    while log.entries.len() > max_entries {
                        log.entries.pop_back();
                    }
                    return log;
                }
            }
        }
        Self::new(max_entries)
    }

    pub fn save(&self) {
        let path = Self::log_path();
        if let Ok(contents) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, contents);
        }
    }

    pub fn add_success_with_commands(&mut self, summary: String, commands: Vec<Command>) {
        self.entries.push_front(LogEntry {
            timestamp: Local::now(),
            summary,
            status: LogStatus::Success,
            error: None,
            commands: Some(commands),
        });
        while self.entries.len() > self.max_entries {
            self.entries.pop_back();
        }
        self.save();
    }
}
```

#### Scenario: Log persistence
- **WHEN** print job completes
- **THEN** entry added to VecDeque, saved to `print_log.json`

#### Scenario: Log trimming
- **WHEN** entries exceed `max_log_entries`
- **THEN** oldest entries removed to stay within limit

#### Scenario: Log loading
- **WHEN** application starts
- **THEN** loads existing entries from `print_log.json`

---

## Preset Reference

| Preset | Vendor ID | Product ID | Endpoint | Interface |
|--------|-----------|------------|----------|-----------|
| Standard | 0x0483 | 0x5840 | Auto | Auto |
| ICS Advent | 0x0FE6 | 0x811E | 1 | 0 |
| Manual | User-defined | User-defined | Optional | Optional |

---

## Configuration File Format

### Example: config.toml (Standard)

```toml
[printer]
preset = "standard"

[server]
port = 55000

[ui]
max_log_entries = 100
logging_enabled = false
```

### Example: config.toml (ICS Advent)

```toml
[printer]
preset = "ics_advent"

[server]
port = 55000

[ui]
max_log_entries = 100
logging_enabled = false
```

### Example: config.toml (Manual)

```toml
[printer]
preset = "manual"
vendor_id = 1155      # 0x0483 in decimal
product_id = 22592    # 0x5840 in decimal
endpoint = 2
interface = 0

[server]
port = 8080

[ui]
max_log_entries = 200
logging_enabled = true
```

---

## File Locations

| File | Location | Purpose |
|------|----------|---------|
| `config.toml` | Executable directory | Application configuration |
| `print_log.json` | Executable directory | Print job history |
| `reika-debug.log` | Executable directory | Debug logs (when enabled) |

---

## Design Decisions

### TOML Format

Chosen over JSON/YAML for:

1. Human-readable and editable
2. Good serde support in Rust
3. Comments allowed (JSON doesn't support)
4. Standard for Rust ecosystem (Cargo.toml)

### Preset System

Presets simplify configuration for common printers:

1. Users select preset instead of finding USB IDs
2. ICS Advent preset handles quirks (fixed endpoint/interface)
3. Manual mode for any ESC/POS printer

### Executable-Relative Paths

Config files stored next to executable:

1. No registry or AppData complexity
2. Portable installation (copy folder)
3. Easy to edit or backup
