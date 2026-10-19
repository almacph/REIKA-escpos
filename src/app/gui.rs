#![allow(
    clippy::items_after_test_module,
    clippy::type_complexity,
    clippy::too_many_arguments
)]

use crate::app::print_log::LogStatus;
use crate::app::{
    is_exit_requested, notify_print_error, notify_print_success, notify_printer_offline,
    notify_printer_online, render_receipt_preview, take_operator_warnings, take_show_requested,
    update_tray_status, AppConfig, LogEntry, PrintLog, PrinterPreset,
};
use crate::models::{Command, Commands};
use crate::services::health::{HealthSnapshot, SystemClock};
use crate::services::PrinterService;
use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

#[cfg(target_os = "windows")]
use windows::core::PCWSTR;
#[cfg(target_os = "windows")]
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, FindWindowW, GetForegroundWindow, GetWindowThreadProcessId,
    SetForegroundWindow, SetWindowPos, ShowWindow, HWND_TOP, SWP_NOMOVE, SWP_NOSIZE,
    SWP_SHOWWINDOW, SW_RESTORE, SW_SHOWNORMAL,
};

/// Force window to foreground on Windows by finding it by title
#[cfg(target_os = "windows")]
fn force_show_window_by_title(title: &str) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    let wide_title: Vec<u16> = OsStr::new(title)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        if let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(wide_title.as_ptr())) {
            if !hwnd.is_invalid() {
                let foreground_hwnd = GetForegroundWindow();
                let foreground_thread = GetWindowThreadProcessId(foreground_hwnd, None);
                let current_thread = GetCurrentThreadId();

                let attached = if foreground_thread != current_thread {
                    let _ = AttachThreadInput(current_thread, foreground_thread, true);
                    true
                } else {
                    false
                };

                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
                let _ = BringWindowToTop(hwnd);
                let _ = SetForegroundWindow(hwnd);
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOP),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
                );

                if attached {
                    let _ = AttachThreadInput(current_thread, foreground_thread, false);
                }
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn force_show_window_by_title(_title: &str) {
    // No-op on non-Windows platforms
}

#[cfg(test)]
mod reprint_action_tests {
    use super::*;
    use crate::models::{BitImage, CashDrawer, CashDrawerPulse};

    fn setup() -> (
        std_mpsc::Sender<Vec<Command>>,
        std_mpsc::Receiver<Vec<Command>>,
        Arc<AtomicBool>,
        Arc<Mutex<Option<Result<(), String>>>>,
    ) {
        let (tx, rx) = std_mpsc::channel();
        (
            tx,
            rx,
            Arc::new(AtomicBool::new(false)),
            Arc::new(Mutex::new(None)),
        )
    }

    #[test]
    fn invalid_saved_receipt_does_not_enqueue_or_create_history_result() {
        let (tx, rx, progress, result) = setup();
        assert!(!request_reprint(
            &[Command::CashDrawerPulse(CashDrawerPulse {
                pin: CashDrawer::Pin2,
                pulse_ms: 125
            })],
            &tx,
            &progress,
            &result,
        ));
        assert!(rx.try_recv().is_err());
        assert!(!progress.load(Ordering::SeqCst));
        assert!(matches!(*result.lock().unwrap(), Some(Err(_))));
    }

    #[test]
    fn image_only_mixed_and_legacy_saved_receipts_enqueue() {
        let (tx, rx, progress, result) = setup();
        let image = Command::BitImage(BitImage {
            width: 8,
            height: 1,
            data: "AA==".into(),
        });
        assert!(request_reprint(
            std::slice::from_ref(&image),
            &tx,
            &progress,
            &result
        ));
        assert!(rx.try_recv().is_ok());
        progress.store(false, Ordering::SeqCst);
        assert!(request_reprint(
            &[Command::Italic(true), image],
            &tx,
            &progress,
            &result
        ));
        assert!(rx.try_recv().is_ok());
        progress.store(false, Ordering::SeqCst);
        assert!(request_reprint(
            &[Command::Writeln("legacy".into())],
            &tx,
            &progress,
            &result
        ));
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn watchdog_lease_is_removed_when_maintenance_setup_fails_or_finishes() {
        let root = std::env::temp_dir().join(format!("reika-gui-lease-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let path = root.join("maintenance.lease");
        {
            let lease = WatchdogLease::create(&root).expect("lease should be created");
            assert!(path.is_file());
            lease.refresh();
        }
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(root);
    }
}

pub struct PrinterApp {
    config: AppConfig,
    print_log: Arc<Mutex<PrintLog>>,
    printer_online: watch::Receiver<bool>,
    health_snapshot: watch::Receiver<HealthSnapshot>,
    last_online_status: bool,
    show_settings: bool,
    settings_preset: PrinterPreset,
    settings_vendor_id: String,
    settings_product_id: String,
    settings_endpoint: String,
    settings_interface: String,
    settings_port: String,
    settings_logging_enabled: bool,
    settings_reika_api_key: String,
    settings_reika_server_url: String,
    tray_active: bool,
    should_exit: bool,
    minimized_to_tray: bool,
    preview_entry: Option<LogEntry>,
    reprint_tx: std_mpsc::Sender<Vec<Command>>,
    reprint_in_progress: Arc<AtomicBool>,
    reprint_result: Arc<Mutex<Option<Result<(), String>>>>,
    operator_warnings: Vec<String>,
    driver_diagnostics: Option<std_mpsc::Receiver<Vec<crate::app::driver_setup::DeviceDiagnostic>>>,
    driver_results: Vec<crate::app::driver_setup::DeviceDiagnostic>,
    driver_selected: Option<usize>,
    driver_repair: Option<std_mpsc::Receiver<crate::app::driver_setup::RepairOutcome>>,
    driver_repair_started: Option<std::time::Instant>,
    driver_repair_status: Option<String>,
    service: PrinterService,
}

fn request_reprint(
    commands: &[Command],
    tx: &std_mpsc::Sender<Vec<Command>>,
    in_progress: &AtomicBool,
    result: &Arc<Mutex<Option<Result<(), String>>>>,
) -> bool {
    let request = Commands {
        commands: commands.to_vec(),
    };
    if let Err(error) = request.validate() {
        if let Ok(mut slot) = result.lock() {
            *slot = Some(Err(format!("Invalid saved receipt: {error}")));
        }
        return false;
    }
    in_progress.store(true, Ordering::SeqCst);
    tx.send(commands.to_vec()).is_ok()
}

struct WatchdogLease(std::path::PathBuf);

impl WatchdogLease {
    fn create(root: &std::path::Path) -> Result<Self, String> {
        std::fs::create_dir_all(root)
            .map_err(|error| format!("could not create maintenance state directory: {error}"))?;
        let path = root.join("maintenance.lease");
        std::fs::write(&path, b"driver-repair\n")
            .map_err(|error| format!("could not create maintenance lease: {error}"))?;
        Ok(Self(path))
    }

    fn refresh(&self) {
        let _ = std::fs::write(&self.0, b"driver-repair\n");
    }
}

impl Drop for WatchdogLease {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

impl PrinterApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        config: AppConfig,
        print_log: Arc<Mutex<PrintLog>>,
        printer_online: watch::Receiver<bool>,
        health_snapshot: watch::Receiver<HealthSnapshot>,
        tray_active: bool,
        reprint_tx: std_mpsc::Sender<Vec<Command>>,
        reprint_in_progress: Arc<AtomicBool>,
        reprint_result: Arc<Mutex<Option<Result<(), String>>>>,
        service: PrinterService,
        setup_requested: bool,
    ) -> Self {
        let settings_vendor_id = config
            .printer
            .vendor_id
            .map(|v| format!("0x{:04X}", v))
            .unwrap_or_default();
        let settings_product_id = config
            .printer
            .product_id
            .map(|v| format!("0x{:04X}", v))
            .unwrap_or_default();
        let settings_endpoint = config
            .printer
            .endpoint
            .map(|v| v.to_string())
            .unwrap_or_default();
        let settings_interface = config
            .printer
            .interface
            .map(|v| v.to_string())
            .unwrap_or_default();
        Self {
            settings_preset: config.printer.preset,
            settings_vendor_id,
            settings_product_id,
            settings_endpoint,
            settings_interface,
            settings_port: config.server.port.to_string(),
            settings_logging_enabled: config.ui.logging_enabled,
            settings_reika_api_key: config.reika.api_key.clone(),
            settings_reika_server_url: config.reika.server_url.clone(),
            config,
            print_log,
            printer_online,
            health_snapshot,
            last_online_status: false,
            show_settings: setup_requested,
            tray_active,
            should_exit: false,
            minimized_to_tray: false,
            preview_entry: None,
            reprint_tx,
            reprint_in_progress,
            reprint_result,
            operator_warnings: take_operator_warnings(),
            driver_diagnostics: None,
            driver_results: Vec::new(),
            driver_selected: None,
            driver_repair: None,
            driver_repair_started: None,
            driver_repair_status: None,
            service,
        }
    }

    fn render_status_panel(&self, ui: &mut egui::Ui) {
        let is_online = *self.printer_online.borrow();
        let health = self.health_snapshot.borrow().refreshed(&SystemClock);

        egui::Frame::group(ui.style())
            .fill(ui.style().visuals.window_fill)
            .show(ui, |ui| {
                ui.heading("Printer Status");
                ui.add_space(8.0);

                if is_online {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("\u{25CF}")
                                .color(egui::Color32::from_rgb(0, 200, 0))
                                .size(20.0),
                        );
                        ui.label(egui::RichText::new("ONLINE").strong().size(18.0));
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("\u{25CF}")
                                .color(egui::Color32::from_rgb(200, 0, 0))
                                .size(20.0),
                        );
                        ui.label(
                            egui::RichText::new("OFFLINE")
                                .strong()
                                .size(18.0)
                                .color(egui::Color32::from_rgb(200, 0, 0)),
                        );
                    });

                    ui.add_space(8.0);
                    egui::Frame::NONE
                        .fill(egui::Color32::from_rgb(60, 30, 30))
                        .inner_margin(8.0)
                        .corner_radius(4.0)
                        .show(ui, |ui: &mut egui::Ui| {
                            ui.label(
                                egui::RichText::new("\u{26A0} Printer Offline")
                                    .color(egui::Color32::from_rgb(255, 200, 100)),
                            );
                            ui.label("Please check that the printer is plugged in");
                            ui.label("and the USB cable is securely connected.");
                        });
                }

                ui.add_space(8.0);
                let preset_name = match self.config.printer.preset {
                    PrinterPreset::Standard => "Standard (XP-58IIH)",
                    PrinterPreset::IcsAdvent => "ICS Advent Adapter",
                    PrinterPreset::Manual => "Manual Config",
                };
                ui.label(preset_name);
                ui.label(format!(
                    "USB: 0x{:04X}:0x{:04X}",
                    self.config.printer.resolved_vendor_id(),
                    self.config.printer.resolved_product_id()
                ));
                ui.label(format!(
                    "Transport: {:?} ({}s ago)",
                    health.transport,
                    health.transport_age_millis / 1000
                ));
                ui.label(format!(
                    "Readiness: {:?} ({})",
                    health.readiness,
                    if health.status_capability
                        == crate::services::health::StatusCapability::Verified
                    {
                        "verified"
                    } else {
                        "unknown"
                    }
                ));
                ui.label(format!(
                    "Reason: {:?} · Generation {}",
                    health.reason, health.generation.0
                ));
                if health.busy {
                    ui.label("Busy or retrying");
                }
            });
    }

    fn render_log_panel(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style())
            .fill(ui.style().visuals.window_fill)
            .show(ui, |ui| {
                ui.heading("Print Log");
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new("Click an entry to preview the receipt")
                        .weak()
                        .small(),
                );
                ui.add_space(4.0);

                let Ok(log) = self.print_log.lock() else {
                    ui.label(egui::RichText::new("Log unavailable").italics().weak());
                    return;
                };

                if log.is_empty() {
                    ui.label(egui::RichText::new("No print jobs yet").italics().weak());
                } else {
                    let entries: Vec<_> = log.entries().cloned().collect();
                    drop(log);

                    egui::ScrollArea::vertical()
                        .max_height(250.0)
                        .show(ui, |ui| {
                            for entry in entries {
                                let has_commands = entry.commands.is_some();

                                let response = ui.horizontal(|ui| {
                                    let time_str = entry.timestamp.format("%H:%M:%S").to_string();
                                    ui.label(egui::RichText::new(&time_str).monospace().weak());

                                    if has_commands {
                                        if ui.link(&entry.summary).clicked() {
                                            self.preview_entry = Some(entry.clone());
                                        }
                                    } else {
                                        ui.label(&entry.summary);
                                    }

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if has_commands {
                                                ui.label(egui::RichText::new("\u{1F4C4}").weak());
                                            }

                                            match entry.status {
                                                LogStatus::Success => {
                                                    ui.label(
                                                        egui::RichText::new("\u{2713}").color(
                                                            egui::Color32::from_rgb(100, 200, 100),
                                                        ),
                                                    );
                                                }
                                                LogStatus::Error => {
                                                    ui.label(
                                                        egui::RichText::new("\u{2717}").color(
                                                            egui::Color32::from_rgb(200, 100, 100),
                                                        ),
                                                    );
                                                }
                                            }
                                        },
                                    );
                                });

                                if has_commands
                                    && response.response.interact(egui::Sense::click()).clicked()
                                {
                                    self.preview_entry = Some(entry);
                                }

                                ui.separator();
                            }
                        });
                }
            });
    }

    fn render_preview_window(&mut self, ctx: &egui::Context) {
        if let Some(entry) = &self.preview_entry.clone() {
            let mut open = true;
            egui::Window::new("Receipt Preview")
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_size([350.0, 450.0])
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&entry.summary).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let time_str = entry.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
                            ui.label(egui::RichText::new(&time_str).weak());
                        });
                    });

                    match entry.status {
                        LogStatus::Success => {
                            ui.label(
                                egui::RichText::new("\u{2713} Printed successfully")
                                    .color(egui::Color32::from_rgb(100, 200, 100)),
                            );
                        }
                        LogStatus::Error => {
                            let error_text = entry.error.as_deref().unwrap_or("Unknown error");
                            ui.label(
                                egui::RichText::new(format!("\u{2717} Error: {}", error_text))
                                    .color(egui::Color32::from_rgb(200, 100, 100)),
                            );
                        }
                    }

                    // Reprint button
                    if entry.commands.is_some() {
                        ui.add_space(4.0);
                        let is_online = *self.printer_online.borrow();
                        let is_reprinting = self.reprint_in_progress.load(Ordering::SeqCst);
                        let can_reprint = is_online && !is_reprinting;

                        let button_text = if is_reprinting {
                            "Reprinting..."
                        } else {
                            "Reprint"
                        };

                        let button = egui::Button::new(button_text);
                        if ui.add_enabled(can_reprint, button).clicked() {
                            if let Some(cmds) = &entry.commands {
                                let _ = request_reprint(
                                    cmds,
                                    &self.reprint_tx,
                                    &self.reprint_in_progress,
                                    &self.reprint_result,
                                );
                            }
                        }

                        if !is_online && !is_reprinting {
                            ui.label(
                                egui::RichText::new("Printer offline")
                                    .weak()
                                    .small()
                                    .color(egui::Color32::from_rgb(200, 100, 100)),
                            );
                        }
                    }

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    if let Some(commands) = &entry.commands {
                        ui.label(egui::RichText::new("Receipt Mockup:").strong());
                        ui.add_space(4.0);
                        render_receipt_preview(ui, commands);
                    } else {
                        ui.label(
                            egui::RichText::new("No receipt data available")
                                .italics()
                                .weak(),
                        );
                    }
                });

            if !open {
                self.preview_entry = None;
            }
        }
    }

    fn poll_reprint_result(&mut self) {
        if let Ok(mut result) = self.reprint_result.lock() {
            if let Some(res) = result.take() {
                match res {
                    Ok(()) => {
                        notify_print_success("Reprint");
                    }
                    Err(e) => {
                        notify_print_error("Reprint", &e);
                    }
                }
            }
        }
    }

    fn render_settings_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("Settings")
            .collapsible(false)
            .resizable(false)
            .min_width(350.0)
            .show(ctx, |ui| {
                ui.heading("Printer Preset");
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.radio_value(
                        &mut self.settings_preset,
                        PrinterPreset::Standard,
                        "Standard",
                    );
                    ui.label("(XP-58IIH, 0x0483:0x5840)");
                });
                ui.horizontal(|ui| {
                    ui.radio_value(
                        &mut self.settings_preset,
                        PrinterPreset::IcsAdvent,
                        "ICS Advent",
                    );
                    ui.label("(0x0FE6:0x811E, EP:1, IF:0)");
                });
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.settings_preset, PrinterPreset::Manual, "Manual");
                    ui.label("(custom settings)");
                });

                ui.add_space(12.0);

                if self.settings_preset == PrinterPreset::Manual {
                    ui.heading("Manual USB Settings");
                    ui.add_space(8.0);

                    egui::Grid::new("manual_settings_grid")
                        .num_columns(2)
                        .spacing([10.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Vendor ID:");
                            ui.text_edit_singleline(&mut self.settings_vendor_id);
                            ui.end_row();

                            ui.label("Product ID:");
                            ui.text_edit_singleline(&mut self.settings_product_id);
                            ui.end_row();

                            ui.label("Endpoint:");
                            ui.text_edit_singleline(&mut self.settings_endpoint);
                            ui.end_row();

                            ui.label("Interface:");
                            ui.text_edit_singleline(&mut self.settings_interface);
                            ui.end_row();
                        });

                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Leave endpoint/interface empty for auto-detect")
                            .weak()
                            .small(),
                    );
                }

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                ui.heading("Server Settings");
                ui.add_space(8.0);

                egui::Grid::new("server_settings_grid")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Server Port:");
                        ui.text_edit_singleline(&mut self.settings_port);
                        ui.end_row();
                    });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                ui.heading("REIKA Integration");
                ui.add_space(8.0);

                egui::Grid::new("reika_settings_grid")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("API Key:");
                        ui.text_edit_singleline(&mut self.settings_reika_api_key);
                        ui.end_row();

                        ui.label("Server URL:");
                        ui.text_edit_singleline(&mut self.settings_reika_server_url);
                        ui.end_row();
                    });

                ui.label(
                    egui::RichText::new("Sensor health reporting to REIKA dashboard")
                        .weak()
                        .small(),
                );

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                ui.heading("Debug Logging");
                ui.add_space(8.0);

                ui.checkbox(
                    &mut self.settings_logging_enabled,
                    "Enable debug logging to file",
                );
                ui.label(
                    egui::RichText::new(format!(
                        "State directory: {}",
                        crate::app::storage::StateDirectory::resolve()
                            .root()
                            .display()
                    ))
                    .weak()
                    .small(),
                );
                if ui.button("Export diagnostics").clicked() {
                    match crate::app::write_diagnostic_export() {
                        Ok(path) => {
                            notify_print_success(&format!("Diagnostics: {}", path.display()))
                        }
                        Err(error) => notify_print_error("Diagnostics", &error.to_string()),
                    }
                }

                ui.add_space(12.0);
                ui.separator();
                ui.heading("Windows USB Setup / Repair");
                ui.label(
                    "Read-only inventory. Setup changes require an explicit elevated installer.",
                );
                if ui.button("Inspect driver bindings").clicked()
                    && self.driver_diagnostics.is_none()
                {
                    let (tx, rx) = std_mpsc::channel();
                    self.driver_diagnostics = Some(rx);
                    std::thread::spawn(move || {
                        let _ = tx.send(crate::app::driver_setup::inventory());
                    });
                }
                if let Some(rx) = &self.driver_diagnostics {
                    if let Ok(results) = rx.try_recv() {
                        self.driver_results = results;
                        self.driver_diagnostics = None;
                    }
                }
                for result in &self.driver_results {
                    ui.label(format!("{} — {}", result.state, result.detail));
                    if !result.hardware_id.is_empty() {
                        ui.small(format!(
                            "{} / {} / {}",
                            result.hardware_id, result.service, result.provider
                        ));
                    }
                }
                if !self.driver_results.is_empty() {
                    ui.add_space(6.0);
                    ui.label("Select the exact enumerated device before repair:");
                    for (index, result) in self.driver_results.iter().enumerate() {
                        let label = if result.instance_id.is_empty() {
                            format!("{} ({})", result.state, result.hardware_id)
                        } else {
                            result.instance_id.clone()
                        };
                        if ui
                            .selectable_label(self.driver_selected == Some(index), label)
                            .clicked()
                        {
                            self.driver_selected = Some(index);
                            self.driver_repair_status = None;
                        }
                    }
                    let selected = self
                        .driver_selected
                        .and_then(|index| self.driver_results.get(index));
                    let can_repair = selected.is_some_and(|device| {
                        !device.instance_id.is_empty()
                            && device.preset.is_some()
                            && !matches!(
                                device.state,
                                crate::app::driver_setup::DriverState::Absent
                                    | crate::app::driver_setup::DriverState::Unsupported
                                    | crate::app::driver_setup::DriverState::Ambiguous
                            )
                    }) && self.driver_repair.is_none();
                    if ui
                        .add_enabled(can_repair, egui::Button::new("Repair selected device (administrator)"))
                        .clicked()
                    {
                        if let Some(device) = selected.cloned() {
                            let instance = device.instance_id;
                            let key = match device.preset.unwrap_or_default() {
                                "ICS Advent" => "ics-advent",
                                _ => "standard",
                            }
                            .to_string();
                            let service = self.service.clone();
                            let (tx, rx) = std_mpsc::channel();
                            self.driver_repair = Some(rx);
                            self.driver_repair_started = Some(std::time::Instant::now());
                            self.driver_repair_status = Some(
                                "Releasing the printer handle before requesting administrator approval…"
                                    .into(),
                            );
                            std::thread::spawn(move || {
                                let runtime = match tokio::runtime::Runtime::new() {
                                    Ok(runtime) => runtime,
                                    Err(error) => {
                                        let _ = tx.send(
                                            crate::app::driver_setup::RepairOutcome::Failed(
                                                format!("could not start repair worker: {error}"),
                                            ),
                                        );
                                        return;
                                    }
                                };
                                let result = runtime.block_on(async move {
                                    let state_dir = crate::app::storage::StateDirectory::resolve();
                                    if !state_dir.is_available() {
                                        return Err(crate::error::AppError::Internal(
                                            state_dir.error().unwrap_or("user state directory unavailable").into(),
                                        ));
                                    }
                                    // Pause watchdog recovery before asking the owner to release
                                    // its handle, closing the small race between those operations.
                                    let watchdog_lease = WatchdogLease::create(state_dir.root())
                                        .map_err(crate::error::AppError::Internal)?;
                                    let lease = match service.enter_maintenance().await {
                                        Ok(lease) => lease,
                                        Err(error) => {
                                            drop(watchdog_lease);
                                            return Err(error);
                                        }
                                    };
                                    let helper = crate::app::driver_setup::launch_repair(instance, key);
                                    let result = loop {
                                        match helper.recv_timeout(std::time::Duration::from_secs(30)) {
                                            Ok(outcome) => break outcome,
                                            Err(std_mpsc::RecvTimeoutError::Timeout) => {
                                                // Keep the watchdog lease fresh while the helper
                                                // remains alive. This also keeps maintenance held
                                                // if Windows denied PROCESS_TERMINATE.
                                                watchdog_lease.refresh();
                                            }
                                            Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                                                break crate::app::driver_setup::RepairOutcome::Failed(
                                                    "elevated helper worker crashed".into(),
                                                )
                                            }
                                        }
                                    };
                                    drop(lease);
                                    drop(watchdog_lease);
                                    Ok::<_, crate::error::AppError>(result)
                                });
                                let outcome = match result {
                                    Ok(outcome) => outcome,
                                    Err(error) => crate::app::driver_setup::RepairOutcome::Failed(
                                        error.to_string(),
                                    ),
                                };
                                let _ = tx.send(outcome);
                            });
                        }
                    }
                    if let Some(rx) = &self.driver_repair {
                        if let Ok(outcome) = rx.try_recv() {
                            self.driver_repair_status = Some(outcome.to_string());
                            self.driver_repair = None;
                            self.driver_repair_started = None;
                            self.driver_diagnostics = None;
                        }
                    }
                    if self.driver_repair.is_some()
                        && self
                            .driver_repair_started
                            .is_some_and(|started| started.elapsed() >= std::time::Duration::from_secs(300))
                    {
                        self.driver_repair_status = Some(
                            "Administrator helper did not exit after 300 seconds; maintenance remains active and manual recovery may be required."
                                .into(),
                        );
                    }
                    if let Some(status) = &self.driver_repair_status {
                        ui.label(status);
                    }
                }

                ui.add_space(16.0);
                ui.label(
                    egui::RichText::new("Note: Changes require application restart")
                        .weak()
                        .italics(),
                );

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        self.config.printer.preset = self.settings_preset;

                        if self.settings_preset == PrinterPreset::Manual {
                            self.config.printer.vendor_id = u16::from_str_radix(
                                self.settings_vendor_id.trim_start_matches("0x"),
                                16,
                            )
                            .ok();
                            self.config.printer.product_id = u16::from_str_radix(
                                self.settings_product_id.trim_start_matches("0x"),
                                16,
                            )
                            .ok();
                            self.config.printer.endpoint = self.settings_endpoint.parse().ok();
                            self.config.printer.interface = self.settings_interface.parse().ok();
                        } else {
                            self.config.printer.vendor_id = None;
                            self.config.printer.product_id = None;
                            self.config.printer.endpoint = None;
                            self.config.printer.interface = None;
                        }

                        if let Ok(port) = self.settings_port.parse() {
                            self.config.server.port = port;
                        }
                        self.config.ui.logging_enabled = self.settings_logging_enabled;
                        self.config.reika.api_key = self.settings_reika_api_key.clone();
                        self.config.reika.server_url = self.settings_reika_server_url.clone();
                        match self.config.save() {
                            Ok(()) => self.show_settings = false,
                            Err(error) => notify_print_error(
                                "Settings",
                                &format!("Could not save configuration: {error}"),
                            ),
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.settings_preset = self.config.printer.preset;
                        self.settings_vendor_id = self
                            .config
                            .printer
                            .vendor_id
                            .map(|v| format!("0x{:04X}", v))
                            .unwrap_or_default();
                        self.settings_product_id = self
                            .config
                            .printer
                            .product_id
                            .map(|v| format!("0x{:04X}", v))
                            .unwrap_or_default();
                        self.settings_endpoint = self
                            .config
                            .printer
                            .endpoint
                            .map(|v| v.to_string())
                            .unwrap_or_default();
                        self.settings_interface = self
                            .config
                            .printer
                            .interface
                            .map(|v| v.to_string())
                            .unwrap_or_default();
                        self.settings_port = self.config.server.port.to_string();
                        self.settings_logging_enabled = self.config.ui.logging_enabled;
                        self.settings_reika_api_key = self.config.reika.api_key.clone();
                        self.settings_reika_server_url = self.config.reika.server_url.clone();
                        self.show_settings = false;
                    }
                });
            });
    }

    fn handle_tray_events(&mut self, ctx: &egui::Context) {
        if take_show_requested() {
            self.show_window(ctx);
        }

        if self.tray_active {
            let is_online = *self.printer_online.borrow();
            if is_online != self.last_online_status {
                update_tray_status(is_online);

                if self.minimized_to_tray {
                    if is_online {
                        notify_printer_online();
                    } else {
                        notify_printer_offline();
                    }
                }

                self.last_online_status = is_online;
            }
        }
    }

    fn show_window(&mut self, ctx: &egui::Context) {
        self.minimized_to_tray = false;
        force_show_window_by_title("REIKA Printer Service");
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(450.0, 500.0)));
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
            100.0, 100.0,
        )));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }
}

impl eframe::App for PrinterApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.request_repaint_after(std::time::Duration::from_millis(100));

        self.handle_tray_events(&ctx);
        self.poll_reprint_result();
        self.operator_warnings.extend(take_operator_warnings());

        if self.should_exit || is_exit_requested() {
            std::process::exit(0);
        }

        if ctx.input(|i| i.viewport().close_requested()) && self.tray_active {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            self.minimized_to_tray = true;
            return;
        }

        egui::Panel::top("header").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("REIKA Printer Service");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let is_online = *self.printer_online.borrow();
                    if is_online {
                        ui.label(
                            egui::RichText::new("\u{25CF} Online")
                                .color(egui::Color32::from_rgb(0, 200, 0)),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new("\u{25CF} Offline")
                                .color(egui::Color32::from_rgb(200, 0, 0)),
                        );
                    }
                });
            });
        });

        if !self.operator_warnings.is_empty() {
            egui::Panel::top("operator-warnings").show(ui, |ui| {
                for warning in &self.operator_warnings {
                    ui.colored_label(
                        egui::Color32::from_rgb(180, 80, 0),
                        format!("Warning: {warning}"),
                    );
                }
                if ui.small_button("Dismiss").clicked() {
                    self.operator_warnings.clear();
                }
            });
        }

        egui::Panel::bottom("footer").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Server: localhost:{}", self.config.server.port));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("\u{2699} Settings").clicked() {
                        self.show_settings = true;
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            self.render_status_panel(ui);
            ui.add_space(8.0);
            self.render_log_panel(ui);
        });

        if self.show_settings {
            self.render_settings_window(&ctx);
        }

        self.render_preview_window(&ctx);
    }
}
