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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterConfig {
    pub vendor_id: u16,
    pub product_id: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub max_log_entries: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        use crate::services::{DEFAULT_VENDOR_ID, DEFAULT_PRODUCT_ID};
        Self {
            printer: PrinterConfig {
                vendor_id: DEFAULT_VENDOR_ID,
                product_id: DEFAULT_PRODUCT_ID,
            },
            server: ServerConfig { port: 55000 },
            ui: UiConfig {
                max_log_entries: 100,
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
            match fs::read_to_string(&path) {
                Ok(contents) => match toml::from_str(&contents) {
                    Ok(config) => return config,
                    Err(e) => eprintln!("Failed to parse config: {}", e),
                },
                Err(e) => eprintln!("Failed to read config: {}", e),
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
