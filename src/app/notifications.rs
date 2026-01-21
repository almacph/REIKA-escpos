use notify_rust::Notification;

const APP_NAME: &str = "REIKA Printer Service";

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
