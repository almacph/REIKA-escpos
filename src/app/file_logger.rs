use super::storage::StateDirectory;
use log::{Level, LevelFilter, Metadata, Record};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::OnceLock;

const LOG_FILENAME: &str = "reika-debug.log";
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
const MAX_ARCHIVES: usize = 3;
const MAX_RECORD_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, serde::Serialize)]
struct DiagnosticEvent {
    timestamp: String,
    state: String,
    category: String,
    attempt_id: String,
}

static DIAGNOSTIC_EVENTS: OnceLock<Mutex<Vec<DiagnosticEvent>>> = OnceLock::new();

fn diagnostic_events() -> &'static Mutex<Vec<DiagnosticEvent>> {
    DIAGNOSTIC_EVENTS.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn record_diagnostic_event(state: &str, category: &str, attempt_id: Option<&str>) {
    if let Ok(mut events) = diagnostic_events().lock() {
        events.push(DiagnosticEvent {
            timestamp: chrono::Local::now().to_rfc3339(),
            state: state.to_string(),
            category: category.to_string(),
            attempt_id: attempt_id.unwrap_or("none").to_string(),
        });
        if events.len() > 100 {
            let keep_from = events.len() - 100;
            events.drain(..keep_from);
        }
    }
}

struct FileLogger {
    file: Mutex<Option<File>>,
    path: PathBuf,
    echo_stderr: bool,
}

impl FileLogger {
    fn new(file: File, path: PathBuf) -> Self {
        Self {
            file: Mutex::new(Some(file)),
            path,
            echo_stderr: true,
        }
    }

    #[cfg(test)]
    fn new_silent(file: File, path: PathBuf) -> Self {
        Self {
            file: Mutex::new(Some(file)),
            path,
            echo_stderr: false,
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

            let mut log_line = format!("[{}] [{}] [{}] {}\n", timestamp, level, target, message);
            if log_line.len() > MAX_RECORD_BYTES {
                let mut end = MAX_RECORD_BYTES.saturating_sub(32);
                while end > 0 && !log_line.is_char_boundary(end) {
                    end -= 1;
                }
                log_line.truncate(end);
                log_line.push_str("... [record truncated]\n");
            }

            // Write to file
            if let Ok(mut slot) = self.file.lock() {
                let needs_rotation = slot
                    .as_ref()
                    .and_then(|file| file.metadata().ok())
                    .is_some_and(|metadata| {
                        metadata.len().saturating_add(log_line.len() as u64) > MAX_LOG_BYTES
                    });
                if needs_rotation {
                    slot.take();
                    let _ = rotate_logs(&self.path);
                    if let Ok(file) = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&self.path)
                    {
                        *slot = Some(file);
                    }
                }
                if let Some(file) = slot.as_mut() {
                    let _ = file.write_all(log_line.as_bytes());
                    let _ = file.flush();
                }
            }

            // Also print to stderr for console visibility
            if self.echo_stderr {
                eprint!("{}", log_line);
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut slot) = self.file.lock() {
            if let Some(file) = slot.as_mut() {
                let _ = file.flush();
            }
        }
    }
}

/// Get the log file path (next to executable)
pub fn log_path() -> PathBuf {
    StateDirectory::resolve().path(LOG_FILENAME)
}

/// Initialize file logging. Creates/truncates the log file.
/// Returns Ok(()) if logging was initialized, Err if it failed.
pub fn init_file_logging() -> Result<(), Box<dyn std::error::Error>> {
    let path = log_path();

    rotate_logs(&path)?;
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| {
            eprintln!("Failed to create log file at {:?}: {}", path, e);
            e
        })?;

    let logger = FileLogger::new(file, path.clone());

    log::set_boxed_logger(Box::new(logger))?;
    log::set_max_level(LevelFilter::Debug);

    log::info!("=== REIKA Printer Service Started ===");
    log::info!("Log file: {:?}", path);
    log::info!("Logging initialized at DEBUG level");

    Ok(())
}

fn rotate_logs(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if path
        .metadata()
        .map(|m| m.len() < MAX_LOG_BYTES)
        .unwrap_or(true)
    {
        return Ok(());
    }
    for index in (1..=MAX_ARCHIVES).rev() {
        let source = if index == 1 {
            path.to_path_buf()
        } else {
            path.with_extension(format!("log.{}", index - 1))
        };
        let destination = path.with_extension(format!("log.{}", index));
        if index == MAX_ARCHIVES {
            let _ = std::fs::remove_file(&destination);
        }
        if source.exists() {
            let _ = std::fs::rename(source, destination);
        }
    }
    for index in (MAX_ARCHIVES + 1)..=1000 {
        let stale = path.with_extension(format!("log.{index}"));
        if !stale.exists() {
            break;
        }
        let _ = std::fs::remove_file(stale);
    }
    Ok(())
}

pub fn diagnostic_export() -> String {
    let mode = match StateDirectory::resolve().mode() {
        super::storage::StorageMode::Portable => "portable",
        super::storage::StorageMode::Installed => "installed",
    };
    let events = diagnostic_events()
        .lock()
        .map(|events| events.clone())
        .unwrap_or_default();
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "storage_mode": mode,
        "state_dir": StateDirectory::resolve().root(),
        "events": events,
    })
    .to_string()
}

pub fn write_diagnostic_export() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let state = StateDirectory::resolve();
    write_diagnostic_export_for_state(&state)
}

fn write_diagnostic_export_for_state(
    state: &StateDirectory,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if !state.is_available() {
        return Err(state
            .error()
            .unwrap_or("state directory unavailable")
            .into());
    }
    let path = state.path("diagnostic-export.json");
    super::storage::atomic_write(&path, diagnostic_export().as_bytes())?;
    Ok(path)
}

/// Initialize a no-op logger (when logging is disabled)
pub fn init_noop_logging() {
    // Set max level to Off so no logging happens
    log::set_max_level(LevelFilter::Off);
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::Log;

    #[test]
    fn runtime_rotation_bounds_active_file_and_archives() {
        let root = std::env::temp_dir().join(format!("reika-logger-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(LOG_FILENAME);
        std::fs::write(path.with_extension("log.4"), b"stale").unwrap();
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap();
        let logger = FileLogger::new_silent(file, path.clone());
        let message = "x".repeat(MAX_RECORD_BYTES);
        for _ in 0..400 {
            let args = format_args!("{message}");
            let record = log::Record::builder()
                .args(args)
                .level(Level::Debug)
                .target("storage-test")
                .build();
            logger.log(&record);
        }
        let active = std::fs::metadata(&path).unwrap().len();
        assert!(active <= MAX_LOG_BYTES);
        for index in 1..=MAX_ARCHIVES {
            assert!(path.with_extension(format!("log.{index}")).exists());
        }
        assert!(!path.with_extension("log.4").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn oversized_multibyte_record_is_bounded_without_panicking() {
        let root = std::env::temp_dir().join(format!("reika-unicode-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(LOG_FILENAME);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap();
        let logger = FileLogger::new_silent(file, path.clone());
        let message = "界".repeat(MAX_RECORD_BYTES);
        let args = format_args!("{message}");
        let record = log::Record::builder()
            .args(args)
            .level(Level::Debug)
            .target("unicode-test")
            .build();
        logger.log(&record);
        let bytes = std::fs::read(path).unwrap();
        assert!(bytes.len() <= MAX_RECORD_BYTES);
        assert!(String::from_utf8(bytes)
            .unwrap()
            .contains("record truncated"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn diagnostic_export_contains_operations_but_redacts_sensitive_data() {
        record_diagnostic_event("print_failed", "usb_timeout", Some("attempt-42"));
        let export = diagnostic_export();
        assert!(export.contains("usb_timeout"));
        assert!(export.contains("attempt-42"));
        assert!(!export.contains("api_key"));
        assert!(!export.contains("Writeln"));
        assert!(!export.contains("receipt"));
    }

    #[test]
    fn unavailable_state_does_not_write_diagnostic_export() {
        let root =
            std::env::temp_dir().join(format!("reika-export-unavailable-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let state = StateDirectory::unavailable_for_test(root.clone());
        assert!(write_diagnostic_export_for_state(&state).is_err());
        assert!(!root.join("diagnostic-export.json").exists());
        let _ = std::fs::remove_dir_all(root);
    }
}
