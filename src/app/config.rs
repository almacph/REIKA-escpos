use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::file_logger::record_diagnostic_event;
use super::notifications::push_operator_warning;
use super::storage::{atomic_write, StateDirectory};

const CONFIG_FILENAME: &str = "config.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub printer: PrinterConfig,
    pub server: ServerConfig,
    pub ui: UiConfig,
    #[serde(default)]
    pub reika: ReikaConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PrinterPreset {
    #[default]
    Standard,
    IcsAdvent,
    Manual,
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
            PrinterPreset::Standard => None,
            PrinterPreset::IcsAdvent => Some(1),
            PrinterPreset::Manual => self.endpoint,
        }
    }

    pub fn resolved_interface(&self) -> Option<u8> {
        match self.preset {
            PrinterPreset::Standard => None,
            PrinterPreset::IcsAdvent => Some(0),
            PrinterPreset::Manual => self.interface,
        }
    }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReikaConfig {
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_reika_server_url")]
    pub server_url: String,
}

fn default_reika_server_url() -> String {
    "https://reika.local".to_string()
}

impl Default for ReikaConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            server_url: default_reika_server_url(),
        }
    }
}

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
            reika: ReikaConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn config_path() -> PathBuf {
        let state = StateDirectory::resolve();
        if state.mode() == super::storage::StorageMode::Installed {
            let portable = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.to_path_buf()));
            if let Some(portable) = portable {
                if let Ok(report) = state.migrate_portable_files(
                    &portable,
                    &[CONFIG_FILENAME, "print_log.json", "reika-debug.log"],
                ) {
                    if !report.is_clean() {
                        record_diagnostic_event("migration_attention", "state_migration", None);
                        push_operator_warning(format!(
                            "State migration needs attention. Conflicts: {:?}; invalid files: {:?}",
                            report.conflicts, report.invalid
                        ));
                    }
                } else {
                    let error = state
                        .error()
                        .map(str::to_owned)
                        .unwrap_or_else(|| "could not migrate portable state".to_string());
                    record_diagnostic_event("state_unavailable", "original_user_handoff", None);
                    push_operator_warning(error);
                }
            }
        }
        state.path(CONFIG_FILENAME)
    }

    pub fn load() -> Self {
        let state = StateDirectory::resolve();
        if !state.is_available() {
            if let Some(error) = state.error() {
                push_operator_warning(error.to_string());
            }
            return Self::default();
        }
        let path = Self::config_path();
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(contents) => {
                    if let Ok(config) = toml::from_str(&contents) {
                        return config;
                    }
                    let corrupt = preserve_corrupt(&path, "toml.corrupt");
                    record_diagnostic_event("config_recovery", "corrupt_config", None);
                    push_operator_warning(format!(
                        "Configuration was invalid and defaults are active; recovery file: {}",
                        corrupt
                    ));
                }
                Err(error) => {
                    record_diagnostic_event("config_read_failed", "config_io", None);
                    push_operator_warning(format!(
                        "Could not read configuration; defaults are active: {error}"
                    ));
                }
            }
        }

        let config = Self::default();
        let _ = config.save();
        config
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let state = StateDirectory::resolve();
        if !state.is_available() {
            return Err(state
                .error()
                .unwrap_or("state directory unavailable")
                .into());
        }
        let path = Self::config_path();
        let contents = toml::to_string_pretty(self)?;
        atomic_write(&path, contents.as_bytes())?;
        Ok(())
    }
}

fn preserve_corrupt(path: &std::path::Path, suffix: &str) -> String {
    let mut target = path.with_extension(suffix);
    for index in 1..1000 {
        if !target.exists() {
            break;
        }
        target = path.with_extension(format!("{suffix}.{index}"));
    }
    match std::fs::rename(path, &target) {
        Ok(()) => target.display().to_string(),
        Err(error) => {
            push_operator_warning(format!(
                "Could not preserve corrupt configuration ({}); recovery is still required",
                error
            ));
            format!("rename failed: {error}")
        }
    }
}
