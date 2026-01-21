use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;

const LOG_FILENAME: &str = "print_log.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Local>,
    pub summary: String,
    pub status: LogStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LogStatus {
    Success,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintLog {
    entries: VecDeque<LogEntry>,
    #[serde(skip)]
    max_entries: usize,
}

impl PrintLog {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            max_entries,
        }
    }

    fn log_path() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
            .join(LOG_FILENAME)
    }

    pub fn load(max_entries: usize) -> Self {
        let path = Self::log_path();
        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(contents) => match serde_json::from_str::<PrintLog>(&contents) {
                    Ok(mut log) => {
                        log.max_entries = max_entries;
                        log.trim();
                        return log;
                    }
                    Err(e) => eprintln!("Failed to parse print log: {}", e),
                },
                Err(e) => eprintln!("Failed to read print log: {}", e),
            }
        }
        Self::new(max_entries)
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::log_path();
        let contents = serde_json::to_string_pretty(self)?;
        fs::write(path, contents)?;
        Ok(())
    }

    pub fn add_entry(&mut self, summary: String, status: LogStatus, error: Option<String>) {
        let entry = LogEntry {
            timestamp: Local::now(),
            summary,
            status,
            error,
        };
        self.entries.push_front(entry);
        self.trim();
        let _ = self.save();
    }

    pub fn add_success(&mut self, summary: String) {
        self.add_entry(summary, LogStatus::Success, None);
    }

    pub fn add_error(&mut self, summary: String, error: String) {
        self.add_entry(summary, LogStatus::Error, Some(error));
    }

    fn trim(&mut self) {
        while self.entries.len() > self.max_entries {
            self.entries.pop_back();
        }
    }

    pub fn entries(&self) -> impl Iterator<Item = &LogEntry> {
        self.entries.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
