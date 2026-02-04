use crate::app::print_log::LogStatus;
use crate::app::{is_exit_requested, notify_printer_offline, notify_printer_online, render_receipt_preview, take_show_requested, update_tray_status, AppConfig, LogEntry, PrintLog, PrinterPreset};
use eframe::egui;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, FindWindowW, GetForegroundWindow, GetWindowThreadProcessId,
    SetForegroundWindow, SetWindowPos, ShowWindow,
    HWND_TOP, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    SW_RESTORE, SW_SHOWNORMAL,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
#[cfg(target_os = "windows")]
use windows::core::PCWSTR;

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
                    HWND_TOP,
                    0, 0, 0, 0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW
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

pub struct PrinterApp {
    config: AppConfig,
    print_log: Arc<Mutex<PrintLog>>,
    printer_online: watch::Receiver<bool>,
    last_online_status: bool,
    show_settings: bool,
    settings_preset: PrinterPreset,
    settings_vendor_id: String,
    settings_product_id: String,
    settings_endpoint: String,
    settings_interface: String,
    settings_port: String,
    settings_logging_enabled: bool,
    tray_active: bool,
    should_exit: bool,
    minimized_to_tray: bool,
    preview_entry: Option<LogEntry>,
}

impl PrinterApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        config: AppConfig,
        print_log: Arc<Mutex<PrintLog>>,
        printer_online: watch::Receiver<bool>,
        tray_active: bool,
    ) -> Self {
        let settings_vendor_id = config.printer.vendor_id
            .map(|v| format!("0x{:04X}", v))
            .unwrap_or_default();
        let settings_product_id = config.printer.product_id
            .map(|v| format!("0x{:04X}", v))
            .unwrap_or_default();
        let settings_endpoint = config.printer.endpoint
            .map(|v| v.to_string())
            .unwrap_or_default();
        let settings_interface = config.printer.interface
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
            config,
            print_log,
            printer_online,
            last_online_status: false,
            show_settings: false,
            tray_active,
            should_exit: false,
            minimized_to_tray: false,
            preview_entry: None,
        }
    }

    fn render_status_panel(&self, ui: &mut egui::Ui) {
        let is_online = *self.printer_online.borrow();

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
                    egui::Frame::none()
                        .fill(egui::Color32::from_rgb(60, 30, 30))
                        .inner_margin(8.0)
                        .rounding(4.0)
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
            });
    }

    fn render_log_panel(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style())
            .fill(ui.style().visuals.window_fill)
            .show(ui, |ui| {
                ui.heading("Print Log");
                ui.add_space(2.0);
                ui.label(egui::RichText::new("Click an entry to preview the receipt").weak().small());
                ui.add_space(4.0);

                let log = self.print_log.lock().unwrap();

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
                                                        egui::RichText::new("\u{2713}")
                                                            .color(egui::Color32::from_rgb(
                                                                100, 200, 100,
                                                            )),
                                                    );
                                                }
                                                LogStatus::Error => {
                                                    ui.label(
                                                        egui::RichText::new("\u{2717}")
                                                            .color(egui::Color32::from_rgb(200, 100, 100)),
                                                    );
                                                }
                                            }
                                        },
                                    );
                                });

                                if has_commands && response.response.interact(egui::Sense::click()).clicked() {
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

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    if let Some(commands) = &entry.commands {
                        ui.label(egui::RichText::new("Receipt Mockup:").strong());
                        ui.add_space(4.0);
                        render_receipt_preview(ui, commands);
                    } else {
                        ui.label(egui::RichText::new("No receipt data available").italics().weak());
                    }
                });

            if !open {
                self.preview_entry = None;
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
                    ui.radio_value(&mut self.settings_preset, PrinterPreset::Standard, "Standard");
                    ui.label("(XP-58IIH, 0x0483:0x5840)");
                });
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.settings_preset, PrinterPreset::IcsAdvent, "ICS Advent");
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

                ui.heading("Debug Logging");
                ui.add_space(8.0);

                ui.checkbox(&mut self.settings_logging_enabled, "Enable debug logging to file");
                ui.label(
                    egui::RichText::new("Logs to reika-debug.log next to executable")
                        .weak()
                        .small(),
                );

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
                            ).ok();
                            self.config.printer.product_id = u16::from_str_radix(
                                self.settings_product_id.trim_start_matches("0x"),
                                16,
                            ).ok();
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
                        let _ = self.config.save();
                        self.show_settings = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.settings_preset = self.config.printer.preset;
                        self.settings_vendor_id = self.config.printer.vendor_id
                            .map(|v| format!("0x{:04X}", v))
                            .unwrap_or_default();
                        self.settings_product_id = self.config.printer.product_id
                            .map(|v| format!("0x{:04X}", v))
                            .unwrap_or_default();
                        self.settings_endpoint = self.config.printer.endpoint
                            .map(|v| v.to_string())
                            .unwrap_or_default();
                        self.settings_interface = self.config.printer.interface
                            .map(|v| v.to_string())
                            .unwrap_or_default();
                        self.settings_port = self.config.server.port.to_string();
                        self.settings_logging_enabled = self.config.ui.logging_enabled;
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
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(100.0, 100.0)));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }
}

impl eframe::App for PrinterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));

        self.handle_tray_events(ctx);

        if self.should_exit || is_exit_requested() {
            std::process::exit(0);
        }

        if ctx.input(|i| i.viewport().close_requested()) {
            if self.tray_active {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                self.minimized_to_tray = true;
                return;
            }
        }

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
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

        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Server: localhost:{}", self.config.server.port));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("\u{2699} Settings").clicked() {
                        self.show_settings = true;
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_status_panel(ui);
            ui.add_space(8.0);
            self.render_log_panel(ui);
        });

        if self.show_settings {
            self.render_settings_window(ctx);
        }

        self.render_preview_window(ctx);
    }
}
