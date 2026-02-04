use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const CONFIG_FILENAME: &str = "config.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub printer: PrinterConfig,
    pub server: ServerConfig,
    pub ui: UiConfig,
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
