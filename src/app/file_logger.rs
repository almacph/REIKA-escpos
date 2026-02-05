use log::{Level, LevelFilter, Metadata, Record};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

const LOG_FILENAME: &str = "reika-debug.log";

struct FileLogger {
    file: Mutex<File>,
}

impl FileLogger {
    fn new(file: File) -> Self {
        Self {
            file: Mutex::new(file),
        }
    }
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        // Filter out noisy tracing span logs and winit internals
        let target = metadata.target();
        if target.starts_with("tracing::span")
            || target.starts_with("winit")
            || target.starts_with("wgpu")
            || target.starts_with("naga")
        {
            return false;
        }
        metadata.level() <= Level::Debug
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let level = record.level();
            let target = record.target();
            let message = record.args();

            let log_line = format!("[{}] [{}] [{}] {}\n", timestamp, level, target, message);

            // Write to file
            if let Ok(mut file) = self.file.lock() {
                let _ = file.write_all(log_line.as_bytes());
                let _ = file.flush();
            }

            // Also print to stderr for console visibility
            eprint!("{}", log_line);
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}

/// Get the log file path (next to executable)
pub fn log_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join(LOG_FILENAME)
}

/// Initialize file logging. Creates/truncates the log file.
/// Returns Ok(()) if logging was initialized, Err if it failed.
pub fn init_file_logging() -> Result<(), Box<dyn std::error::Error>> {
    let path = log_path();

    // Create or truncate the log file
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .map_err(|e| {
            eprintln!("Failed to create log file at {:?}: {}", path, e);
            e
        })?;

    let logger = FileLogger::new(file);

    log::set_boxed_logger(Box::new(logger))?;
    log::set_max_level(LevelFilter::Debug);

    log::info!("=== REIKA Printer Service Started ===");
    log::info!("Log file: {:?}", path);
    log::info!("Logging initialized at DEBUG level");

    Ok(())
}

/// Initialize a no-op logger (when logging is disabled)
pub fn init_noop_logging() {
    // Set max level to Off so no logging happens
    log::set_max_level(LevelFilter::Off);
}
