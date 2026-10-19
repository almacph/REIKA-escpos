use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;

use super::file_logger::record_diagnostic_event;
use super::notifications::push_operator_warning;
use super::storage::{atomic_write, StateDirectory};
use crate::models::Command;

const LOG_FILENAME: &str = "print_log.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Local>,
    pub summary: String,
    pub status: LogStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The print commands that were executed (for receipt preview)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands: Option<Vec<Command>>,
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
        StateDirectory::resolve().path(LOG_FILENAME)
    }

    pub fn load(max_entries: usize) -> Self {
        if !StateDirectory::resolve().is_available() {
            return Self::new(max_entries);
        }
        let path = Self::log_path();
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(contents) => match serde_json::from_str::<PrintLog>(&contents) {
                    Ok(mut log) => {
                        log.max_entries = max_entries;
                        log.trim();
                        return log;
                    }
                    Err(_e) => {
                        let recovery = preserve_corrupt(&path, "json.corrupt");
                        record_diagnostic_event("history_recovery", "corrupt_print_history", None);
                        push_operator_warning(format!(
                            "Print history was invalid and was preserved at {recovery}"
                        ));
                    }
                },
                Err(e) => {
                    record_diagnostic_event("history_read_failed", "print_history", None);
                    push_operator_warning(format!("Could not read print history: {e}"));
                }
            }
        }
        Self::new(max_entries)
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        if !StateDirectory::resolve().is_available() {
            return Err("state directory unavailable".into());
        }
        let path = Self::log_path();
        let contents = serde_json::to_string_pretty(self)?;
        atomic_write(&path, contents.as_bytes())?;
        Ok(())
    }

    pub fn add_entry(
        &mut self,
        summary: String,
        status: LogStatus,
        error: Option<String>,
        commands: Option<Vec<Command>>,
    ) {
        let entry = LogEntry {
            timestamp: Local::now(),
            summary,
            status,
            error,
            commands,
        };
        self.entries.push_front(entry);
        self.trim();
        if let Err(error) = self.save() {
            record_diagnostic_event("persistence_failed", "print_history", None);
            crate::app::notify_print_error("Print history", &error.to_string());
            eprintln!("Failed to persist print history; receipt will not be resent: {error}");
        }
    }

    pub fn add_success(&mut self, summary: String) {
        self.add_entry(summary, LogStatus::Success, None, None);
    }

    pub fn add_success_with_commands(&mut self, summary: String, commands: Vec<Command>) {
        self.add_entry(summary, LogStatus::Success, None, Some(commands));
    }

    pub fn add_error(&mut self, summary: String, error: String) {
        self.add_entry(summary, LogStatus::Error, Some(error), None);
    }

    pub fn add_error_with_commands(
        &mut self,
        summary: String,
        error: String,
        commands: Vec<Command>,
    ) {
        self.add_entry(summary, LogStatus::Error, Some(error), Some(commands));
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
                "Could not preserve corrupt print history ({}); recovery is still required",
                error
            ));
            format!("rename failed: {error}")
        }
    }
}
