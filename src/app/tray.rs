use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::thread;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder, TrayIconEvent,
};

static EXIT_REQUESTED: AtomicBool = AtomicBool::new(false);
static SHOW_REQUESTED: AtomicBool = AtomicBool::new(false);
static TRAY_ONLINE: AtomicBool = AtomicBool::new(false);
static STATUS_UPDATE_REQUESTED: AtomicBool = AtomicBool::new(false);
static EXIT_MENU_ID: OnceLock<MenuId> = OnceLock::new();
static SHOW_MENU_ID: OnceLock<MenuId> = OnceLock::new();
static TRAY_READY: AtomicBool = AtomicBool::new(false);

/// Check if exit has been requested
pub fn is_exit_requested() -> bool {
    EXIT_REQUESTED.load(Ordering::SeqCst)
}

/// Check if show has been requested and clear the flag
pub fn take_show_requested() -> bool {
    SHOW_REQUESTED.swap(false, Ordering::SeqCst)
}

/// Request application exit
fn request_exit() {
    EXIT_REQUESTED.store(true, Ordering::SeqCst);
}

/// Request window to be shown
fn request_show() {
    SHOW_REQUESTED.store(true, Ordering::SeqCst);
}


/// Update tray icon status (called from GUI thread)
pub fn update_tray_status(online: bool) {
    let prev = TRAY_ONLINE.swap(online, Ordering::SeqCst);
    if prev != online {
        STATUS_UPDATE_REQUESTED.store(true, Ordering::SeqCst);
    }
}

fn create_icon(online: bool) -> tray_icon::Icon {
    let (r, g, b) = if online {
        (0u8, 180u8, 0u8)
    } else {
        (180u8, 0u8, 0u8)
    };

    let size = 32u32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);

    for y in 0..size {
        for x in 0..size {
            let cx = (x as i32) - (size as i32 / 2);
            let cy = (y as i32) - (size as i32 / 2);
            let dist_sq = cx * cx + cy * cy;
            let radius_sq = ((size / 2 - 2) * (size / 2 - 2)) as i32;

            if dist_sq <= radius_sq {
                rgba.push(r);
                rgba.push(g);
                rgba.push(b);
                rgba.push(255);
            } else {
                rgba.push(0);
                rgba.push(0);
                rgba.push(0);
                rgba.push(0);
            }
        }
    }

    tray_icon::Icon::from_rgba(rgba, size, size).expect("Failed to create icon")
}

/// Start the system tray on a dedicated background thread with its own message pump.
/// This is necessary on Windows because tray icon events are delivered via Windows messages.
fn start_tray_thread() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    thread::Builder::new()
        .name("tray-message-pump".to_string())
        .spawn(move || {
            // Create menu items
            let show_item = MenuItem::new("Show Window", true, None);
            let exit_item = MenuItem::new("Exit", true, None);

            // Store menu IDs for event matching
            let show_id = show_item.id().clone();
            let exit_id = exit_item.id().clone();
            let _ = SHOW_MENU_ID.set(show_id);
            let _ = EXIT_MENU_ID.set(exit_id);

            // Build menu
            let menu = Menu::new();
            let _ = menu.append(&show_item);
            let _ = menu.append(&PredefinedMenuItem::separator());
            let _ = menu.append(&exit_item);

            // Create tray icon
            let icon = create_icon(false);
            let tray_icon = match TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_tooltip("REIKA Printer Service - Offline")
                .with_icon(icon)
                .build()
            {
                Ok(tray) => tray,
                Err(_) => return,
            };

            TRAY_READY.store(true, Ordering::SeqCst);

            // Run message pump loop
            #[cfg(target_os = "windows")]
            {
                run_windows_message_loop(&tray_icon);
            }

            #[cfg(not(target_os = "windows"))]
            {
                run_polling_loop(&tray_icon);
            }
        })?;

    Ok(())
}

/// Windows-specific message pump loop
#[cfg(target_os = "windows")]
fn run_windows_message_loop(tray_icon: &TrayIcon) {
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, TranslateMessage, MSG, PeekMessageW, PM_REMOVE,
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

        // Process tray icon click events
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            match &event {
                TrayIconEvent::Click { button, button_state, .. } => {
                    if matches!(button, tray_icon::MouseButton::Left)
                        && matches!(button_state, tray_icon::MouseButtonState::Up)
                    {
                        request_show();
                    }
                }
                TrayIconEvent::DoubleClick { button, .. } => {
                    if matches!(button, tray_icon::MouseButton::Left) {
                        request_show();
                    }
                }
                _ => {}
            }
        }

        // Process menu events
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if let Some(exit_id) = EXIT_MENU_ID.get() {
                if &event.id == exit_id {
                    request_exit();
                }
            }
            if let Some(show_id) = SHOW_MENU_ID.get() {
                if &event.id == show_id {
                    request_show();
                }
            }
        }

        // Check for status update requests
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

        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Non-Windows polling loop (fallback)
#[cfg(not(target_os = "windows"))]
fn run_polling_loop(tray_icon: &TrayIcon) {
    loop {
        if EXIT_REQUESTED.load(Ordering::SeqCst) {
            break;
        }

        // Process tray icon events
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            match &event {
                TrayIconEvent::Click { button, button_state, .. } => {
                    if matches!(button, tray_icon::MouseButton::Left)
                        && matches!(button_state, tray_icon::MouseButtonState::Up)
                    {
                        request_show();
                    }
                }
                TrayIconEvent::DoubleClick { button, .. } => {
                    if matches!(button, tray_icon::MouseButton::Left) {
                        request_show();
                    }
                }
                _ => {}
            }
        }

        // Process menu events
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if let Some(exit_id) = EXIT_MENU_ID.get() {
                if &event.id == exit_id {
                    request_exit();
                }
            }
            if let Some(show_id) = SHOW_MENU_ID.get() {
                if &event.id == show_id {
                    request_show();
                }
            }
        }

        // Check for status update requests
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

        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// SystemTray marker struct - tray runs on dedicated thread
pub struct SystemTray;

impl SystemTray {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        if !TRAY_READY.load(Ordering::SeqCst) {
            start_tray_thread().map_err(|e| -> Box<dyn std::error::Error> {
                Box::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
            })?;

            // Wait for tray to be ready (with timeout)
            for _ in 0..50 {
                if TRAY_READY.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        Ok(Self)
    }
}
