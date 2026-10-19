use notify_rust::Notification;
use std::sync::{Mutex, OnceLock};

const APP_NAME: &str = "REIKA Printer Service";

static OPERATOR_WARNINGS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

fn warnings() -> &'static Mutex<Vec<String>> {
    OPERATOR_WARNINGS.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn push_operator_warning(message: impl Into<String>) {
    let message = message.into();
    if let Ok(mut queue) = warnings().lock() {
        if !queue.iter().any(|existing| existing == &message) {
            queue.push(message.clone());
        }
    }
    let _ = Notification::new()
        .appname(APP_NAME)
        .summary("REIKA needs attention")
        .body(&message)
        .timeout(7000)
        .show();
}

pub fn take_operator_warnings() -> Vec<String> {
    warnings()
        .lock()
        .map(|mut queue| queue.drain(..).collect())
        .unwrap_or_default()
}

pub fn notify_print_success(summary: &str) {
    let _ = Notification::new()
        .appname(APP_NAME)
        .summary("Print Completed")
        .body(&format!("{} printed successfully", summary))
        .timeout(3000)
        .show();
}

pub fn notify_print_error(summary: &str, error: &str) {
    let _ = Notification::new()
        .appname(APP_NAME)
        .summary("Print Failed")
        .body(&format!("{}: {}", summary, error))
        .timeout(5000)
        .show();
}

pub fn notify_printer_online() {
    let _ = Notification::new()
        .appname(APP_NAME)
        .summary("Printer Connected")
        .body("The printer is now online and ready")
        .timeout(3000)
        .show();
}

pub fn notify_printer_offline() {
    let _ = Notification::new()
        .appname(APP_NAME)
        .summary("Printer Disconnected")
        .body("Please check the USB connection")
        .timeout(5000)
        .show();
}
