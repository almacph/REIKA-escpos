# GUI Specification

## Purpose

Windows GUI application with system tray integration using egui/eframe.
## Requirements
### Requirement: Single Instance Enforcement

The system SHALL prevent multiple instances using a Windows named mutex.

**Implementation:** `src/app/single_instance.rs`

```rust
const MUTEX_NAME: &str = "Global\\REIKA_ESCPOS_PRINTER_SERVICE";

#[cfg(windows)]
impl SingleInstance {
    pub fn acquire() -> Result<Self, SingleInstanceError> {
        let wide_name: Vec<u16> = MUTEX_NAME.encode_utf16().chain(std::iter::once(0)).collect();

        unsafe {
            // Check if mutex already exists
            if let Ok(existing) = OpenMutexW(MUTEX_ALL_ACCESS, false, PCWSTR(wide_name.as_ptr())) {
                let _ = CloseHandle(existing);
                return Err(SingleInstanceError::AlreadyRunning);
            }

            // Create new mutex with ownership
            let handle = CreateMutexW(None, true, PCWSTR(wide_name.as_ptr()))?;
            Ok(Self { handle })
        }
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.handle);
            let _ = CloseHandle(self.handle);
        }
    }
}
```

#### Scenario: First instance starts
- **WHEN** no existing instance is running
- **THEN** mutex is created and application starts normally

#### Scenario: Second instance blocked
- **WHEN** attempting to start while another instance runs
- **THEN** shows dialog "REIKA Printer Service is already running"

#### Scenario: Mutex released on exit
- **WHEN** application closes
- **THEN** Drop trait releases mutex allowing new instance

---

### Requirement: Main Window Layout

The system SHALL display a 450x500 pixel window with status and log panels.

**Implementation:** `src/app/gui.rs:50-150`

```rust
impl eframe::App for PrinterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Header panel
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("REIKA Printer Service");
                // Online indicator (green/red circle)
                let (color, text) = if self.last_online_status {
                    (egui::Color32::from_rgb(0, 200, 0), "ONLINE")
                } else {
                    (egui::Color32::from_rgb(200, 0, 0), "OFFLINE")
                };
                ui.painter().circle_filled(/*...*/);
                ui.label(text);
            });
        });

        // Central panel with status and log
        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_status_panel(ui);
            ui.separator();
            self.render_log_panel(ui);
        });

        // Footer panel
        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Server: localhost:{}", self.config.server.port));
                if ui.button("Settings").clicked() {
                    self.show_settings = true;
                }
            });
        });
    }
}
```

#### Scenario: Window dimensions
- **WHEN** application starts
- **THEN** window opens at 450x500 pixels with 400x400 minimum

#### Scenario: Status indicator
- **WHEN** printer online
- **THEN** green circle with "ONLINE" text in header

#### Scenario: Status indicator offline
- **WHEN** printer disconnected
- **THEN** red circle with "OFFLINE" text in header

---

### Requirement: Printer Status Panel

The system SHALL display current printer configuration and connection details.

**Implementation:** `src/app/gui.rs:160-200`

```rust
fn render_status_panel(&self, ui: &mut egui::Ui) {
    ui.heading("Printer Status");

    // Preset name
    let preset_name = match self.config.printer.preset {
        PrinterPreset::Standard => "Standard",
        PrinterPreset::IcsAdvent => "ICS Advent",
        PrinterPreset::Manual => "Manual",
    };
    ui.label(format!("Preset: {}", preset_name));

    // USB identifiers
    ui.label(format!(
        "USB: 0x{:04X}:0x{:04X}",
        self.config.printer.resolved_vendor_id(),
        self.config.printer.resolved_product_id()
    ));

    // Endpoint info (if manual)
    if let Some(ep) = self.config.printer.resolved_endpoint() {
        ui.label(format!("Endpoint: 0x{:02X}", ep));
    }
}
```

#### Scenario: Standard preset display
- **WHEN** using Standard preset
- **THEN** shows "Preset: Standard" and "USB: 0x0483:0x5840"

#### Scenario: Manual preset display
- **WHEN** using Manual preset with custom endpoint
- **THEN** shows "Preset: Manual", custom VID:PID, and endpoint

---

### Requirement: Print Log Panel

The system SHALL display scrollable print history with clickable entries.

**Implementation:** `src/app/gui.rs:202-280`

```rust
fn render_log_panel(&mut self, ui: &mut egui::Ui) {
    ui.heading("Print Log");

    egui::ScrollArea::vertical().show(ui, |ui| {
        if let Ok(log) = self.print_log.lock() {
            for entry in log.entries() {
                let timestamp = entry.timestamp.format("%H:%M:%S");
                let status_icon = match entry.status {
                    LogStatus::Success => "\u{2713}", // Checkmark
                    LogStatus::Error => "\u{2717}",   // X mark
                };
                let status_color = match entry.status {
                    LogStatus::Success => egui::Color32::from_rgb(0, 180, 0),
                    LogStatus::Error => egui::Color32::from_rgb(180, 0, 0),
                };

                if ui.selectable_label(false, format!(
                    "[{}] {} {}",
                    timestamp, entry.summary, status_icon
                )).clicked() {
                    // Show preview window for this entry
                    self.preview_entry = Some(entry.clone());
                }
            }
        }
    });
}
```

#### Scenario: Log entry display
- **WHEN** print jobs have been executed
- **THEN** shows `[HH:MM:SS] Summary ✓` or `[HH:MM:SS] Summary ✗`

#### Scenario: Click to preview
- **WHEN** user clicks log entry
- **THEN** opens receipt preview window showing commands

#### Scenario: Persistent log
- **WHEN** application restarts
- **THEN** previous log entries are loaded from `print_log.json`

---

### Requirement: Receipt Preview Window

The system SHALL render receipt mockup from logged commands.

**Implementation:** `src/app/gui.rs:282-350`, `src/app/receipt_renderer.rs`

```rust
fn render_preview_window(&mut self, ctx: &egui::Context) {
    if let Some(entry) = &self.preview_entry {
        egui::Window::new("Receipt Preview")
            .default_size([350.0, 450.0])
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(format!("Job: {}", entry.summary));
                ui.label(format!("Time: {}", entry.timestamp.format("%Y-%m-%d %H:%M:%S")));

                match entry.status {
                    LogStatus::Success => ui.colored_label(Color32::GREEN, "Status: Success"),
                    LogStatus::Error => {
                        ui.colored_label(Color32::RED, "Status: Error");
                        if let Some(err) = &entry.error {
                            ui.label(format!("Error: {}", err));
                        }
                    }
                };

                if let Some(commands) = &entry.commands {
                    let mut renderer = ReceiptRenderer::new();
                    renderer.process_commands(commands);
                    renderer.render(ui);
                }

                if ui.button("Close").clicked() {
                    self.preview_entry = None;
                }
            });
    }
}
```

#### Scenario: Receipt rendering
- **WHEN** preview window opens for entry with commands
- **THEN** renders thermal receipt mockup with formatting

#### Scenario: Error entry preview
- **WHEN** preview window opens for failed job
- **THEN** shows error message along with attempted commands

---

### Requirement: Settings Window

The system SHALL provide settings dialog for configuration.

**Implementation:** `src/app/gui.rs:352-480`

```rust
fn render_settings_window(&mut self, ctx: &egui::Context) {
    egui::Window::new("Settings")
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            // Printer Preset Selection
            ui.heading("Printer");
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.settings_preset, PrinterPreset::Standard, "Standard");
                ui.radio_value(&mut self.settings_preset, PrinterPreset::IcsAdvent, "ICS Advent");
                ui.radio_value(&mut self.settings_preset, PrinterPreset::Manual, "Manual");
            });

            // Manual settings (shown when Manual selected)
            if self.settings_preset == PrinterPreset::Manual {
                ui.label("Vendor ID (hex):");
                ui.text_edit_singleline(&mut self.settings_vendor_id);
                ui.label("Product ID (hex):");
                ui.text_edit_singleline(&mut self.settings_product_id);
                ui.label("Endpoint (optional):");
                ui.text_edit_singleline(&mut self.settings_endpoint);
                ui.label("Interface (optional):");
                ui.text_edit_singleline(&mut self.settings_interface);
            }

            // Server settings
            ui.heading("Server");
            ui.label("Port:");
            ui.text_edit_singleline(&mut self.settings_port);

            // Debug logging
            ui.heading("Debug");
            ui.checkbox(&mut self.settings_logging_enabled, "Enable file logging");

            // Buttons
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    self.save_settings();
                    self.show_settings = false;
                }
                if ui.button("Cancel").clicked() {
                    self.show_settings = false;
                }
            });
        });
}
```

#### Scenario: Preset selection
- **WHEN** user selects different preset
- **THEN** radio buttons reflect selection, save updates config.toml

#### Scenario: Manual configuration
- **WHEN** Manual preset selected
- **THEN** shows text fields for VID, PID, endpoint, interface

#### Scenario: Settings persistence
- **WHEN** user clicks Save
- **THEN** config.toml is updated and reloaded

---

### Requirement: Minimize to System Tray

The system SHALL minimize to tray instead of closing when window X is clicked.

**Implementation:** `src/app/gui.rs:100-120`

```rust
if ctx.input(|i| i.viewport().close_requested()) {
    if self.tray_active {
        // Cancel close and minimize instead
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        self.minimized_to_tray = true;
        return;
    }
}
```

#### Scenario: Close button minimizes
- **WHEN** user clicks window X button
- **THEN** window minimizes to tray (does not exit)

#### Scenario: Exit only via tray
- **WHEN** user wants to exit application
- **THEN** must right-click tray → Exit

---

### Requirement: System Tray Integration

The system SHALL provide system tray icon with context menu.

**Implementation:** `src/app/tray.rs`

```rust
static EXIT_REQUESTED: AtomicBool = AtomicBool::new(false);
static SHOW_REQUESTED: AtomicBool = AtomicBool::new(false);
static TRAY_ONLINE: AtomicBool = AtomicBool::new(false);

fn create_icon(online: bool) -> tray_icon::Icon {
    let (r, g, b) = if online {
        (0u8, 180u8, 0u8)    // Green
    } else {
        (180u8, 0u8, 0u8)    // Red
    };

    // Generate 32x32 circle icon
    let size = 32u32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let cx = (x as i32) - (size as i32 / 2);
            let cy = (y as i32) - (size as i32 / 2);
            let dist_sq = cx * cx + cy * cy;
            let radius_sq = ((size / 2 - 2) * (size / 2 - 2)) as i32;

            if dist_sq <= radius_sq {
                rgba.extend_from_slice(&[r, g, b, 255]);
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, size, size).expect("Failed to create icon")
}

fn start_tray_thread() {
    thread::spawn(|| {
        let show_item = MenuItem::new("Show Window", true, None);
        let exit_item = MenuItem::new("Exit", true, None);
        let menu = Menu::new();
        menu.append(&show_item);
        menu.append(&PredefinedMenuItem::separator());
        menu.append(&exit_item);

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("REIKA Printer Service - Offline")
            .with_icon(create_icon(false))
            .build()?;

        // Windows message pump loop
        run_windows_message_loop(&tray_icon);
    });
}
```

#### Scenario: Tray icon online
- **WHEN** printer connected
- **THEN** tray icon is green circle, tooltip "REIKA Printer Service - Online"

#### Scenario: Tray icon offline
- **WHEN** printer disconnected
- **THEN** tray icon is red circle, tooltip "REIKA Printer Service - Offline"

#### Scenario: Tray menu - Show Window
- **WHEN** user clicks "Show Window" in tray menu
- **THEN** main window restores and comes to foreground

#### Scenario: Tray menu - Exit
- **WHEN** user clicks "Exit" in tray menu
- **THEN** application closes completely

#### Scenario: Left-click tray icon
- **WHEN** user left-clicks tray icon
- **THEN** main window shows (same as "Show Window")

---

### Requirement: Windows Message Pump

The system SHALL run dedicated thread for tray events with Windows message pump.

**Implementation:** `src/app/tray.rs:150-220`

```rust
#[cfg(target_os = "windows")]
fn run_windows_message_loop(tray_icon: &TrayIcon) {
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
    };

    loop {
        if EXIT_REQUESTED.load(Ordering::SeqCst) {
            break;
        }

        // Process Windows messages
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        // Process tray events
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            match &event {
                TrayIconEvent::Click { button: MouseButton::Left, .. } => {
                    request_show();
                }
                TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } => {
                    request_show();
                }
                _ => {}
            }
        }

        // Process menu events
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if &event.id == EXIT_MENU_ID.get().unwrap() {
                request_exit();
            }
            if &event.id == SHOW_MENU_ID.get().unwrap() {
                request_show();
            }
        }

        // Update icon/tooltip on status change
        if STATUS_UPDATE_REQUESTED.swap(false, Ordering::SeqCst) {
            let online = TRAY_ONLINE.load(Ordering::SeqCst);
            let icon = create_icon(online);
            let tooltip = if online {
                "REIKA Printer Service - Online"
            } else {
                "REIKA Printer Service - Offline"
            };
            let _ = tray_icon.set_icon(Some(icon));
            let _ = tray_icon.set_tooltip(Some(tooltip));
        }

        std::thread::sleep(Duration::from_millis(50));
    }
}
```

#### Scenario: Event processing loop
- **WHEN** tray thread runs
- **THEN** processes Windows messages, tray events, and menu events every 50ms

#### Scenario: Status update
- **WHEN** `update_tray_status(online)` called from printer service
- **THEN** icon and tooltip update on next loop iteration

---

### Requirement: Force Window to Foreground

The system SHALL use Windows API to bring window to foreground reliably.

**Implementation:** `src/app/gui.rs:500-550`

```rust
#[cfg(target_os = "windows")]
fn force_show_window_by_title(title: &str) {
    use windows::Win32::UI::WindowsAndMessaging::*;

    let wide_title: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        let hwnd = FindWindowW(None, PCWSTR(wide_title.as_ptr()));
        if hwnd.0 == 0 { return; }

        let foreground = GetForegroundWindow();
        let foreground_thread = GetWindowThreadProcessId(foreground, None);
        let current_thread = GetCurrentThreadId();

        // Attach to foreground thread to gain focus permission
        let _ = AttachThreadInput(current_thread, foreground_thread, true);

        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);

        let _ = AttachThreadInput(current_thread, foreground_thread, false);
    }
}
```

#### Scenario: Restore from minimized
- **WHEN** window is minimized and "Show Window" clicked
- **THEN** window restores, comes to front, and receives focus

#### Scenario: Thread input attachment
- **WHEN** another application has focus
- **THEN** attaches to foreground thread to gain focus permission

---

### Requirement: Reprint Button in Receipt Preview

The receipt preview window SHALL display a "Reprint" button below the receipt status information and above the receipt mockup.

The button SHALL only be enabled when:
- The preview entry has stored commands (`commands` field is `Some`)
- The printer is currently online

When clicked, the button SHALL send the stored commands through the reprint execution flow (with marker injection) without logging the operation to the print log.

While the reprint is in progress, the button SHALL be disabled and display "Reprinting..." to prevent duplicate submissions.

After completion, a Windows toast notification SHALL indicate success or failure.

#### Scenario: Reprint from preview window
- **WHEN** an operator views a historical receipt in the preview window and clicks "Reprint"
- **THEN** the stored commands are sent to the printer with reprint markers injected at top, middle, and bottom

#### Scenario: Reprint button disabled when offline
- **WHEN** the printer is offline and the operator views a receipt preview
- **THEN** the "Reprint" button is grayed out and not clickable

#### Scenario: Reprint button disabled for entries without commands
- **WHEN** the operator views a log entry that has no stored commands (e.g., a test print)
- **THEN** the "Reprint" button is not displayed

### Requirement: REIKA Sensor Settings

The settings window SHALL include a "REIKA Integration" section with the following fields:
- **API Key**: Text input for the 64-character hex sensor API key
- **Server URL**: Text input for the REIKA server base URL (defaults to `https://reika.local`)

These values SHALL be persisted to `config.toml` under a `[reika]` section and loaded on application startup.

The settings section SHALL display a brief description: "Connect to REIKA sensor dashboard for health monitoring".

When the API key is empty, the section SHALL display a note: "Leave empty to disable sensor reporting".

#### Scenario: Configure REIKA integration
- **WHEN** the operator enters an API key and server URL in the REIKA Integration settings and clicks Save
- **THEN** the values are persisted to `config.toml` under `[reika]` and a restart notice is shown

#### Scenario: Clear REIKA integration
- **WHEN** the operator clears the API key field and saves settings
- **THEN** the sensor reporter is disabled on next restart and no reports are sent

## Design Decisions

### Dedicated Tray Thread

System tray runs on dedicated `tray-message-pump` thread because:

1. Windows tray events require message pump on owning thread
2. egui/eframe main loop can't process tray messages
3. Atomic bools provide lock-free cross-thread signaling

### Dynamic Icon Generation

Icons generated programmatically rather than from files:

1. No external icon file dependency
2. Status-based color change (green/red)
3. Simple 32x32 circle is clear at tray size

### Exit Only Via Tray

Window close button minimizes instead of exiting:

1. Prevents accidental service shutdown
2. Service continues running for POS operations
3. Matches user expectation for "system tray" apps

### Print Log Persistence

Log stored in `print_log.json`:

1. Survives application restarts
2. Enables receipt preview for past jobs
3. Audit trail for troubleshooting
