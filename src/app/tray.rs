use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder,
};

static EXIT_REQUESTED: AtomicBool = AtomicBool::new(false);
static SHOW_REQUESTED: AtomicBool = AtomicBool::new(false);
static EXIT_MENU_ID: OnceLock<MenuId> = OnceLock::new();
static SHOW_MENU_ID: OnceLock<MenuId> = OnceLock::new();

/// Check if exit has been requested (can be called without tray mutex)
pub fn is_exit_requested() -> bool {
    EXIT_REQUESTED.load(Ordering::SeqCst)
}

/// Check if show has been requested and clear the flag
pub fn take_show_requested() -> bool {
    SHOW_REQUESTED.swap(false, Ordering::SeqCst)
}

/// Request application exit (can be called without tray mutex)
fn request_exit() {
    EXIT_REQUESTED.store(true, Ordering::SeqCst);
}

/// Request window to be shown (can be called without tray mutex)
fn request_show() {
    SHOW_REQUESTED.store(true, Ordering::SeqCst);
}

/// Poll menu events without holding the tray mutex.
/// Sets atomic flags for exit and show events.
pub fn poll_tray_menu_events() {
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
}

pub struct SystemTray {
    _tray_icon: TrayIcon,
}

impl SystemTray {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let show_item = MenuItem::new("Show Window", true, None);
        let exit_item = MenuItem::new("Exit", true, None);

        // Store menu IDs in static for lock-free access
        let _ = SHOW_MENU_ID.set(show_item.id().clone());
        let _ = EXIT_MENU_ID.set(exit_item.id().clone());

        let menu = Menu::new();
        menu.append(&show_item)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&exit_item)?;

        let icon = Self::create_icon(false);

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("REIKA Printer Service - Offline")
            .with_icon(icon)
            .build()?;

        Ok(Self {
            _tray_icon: tray_icon,
        })
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

    pub fn update_status(&mut self, online: bool) {
        let icon = Self::create_icon(online);
        let tooltip = if online {
            "REIKA Printer Service - Online"
        } else {
            "REIKA Printer Service - Offline"
        };

        let _ = self._tray_icon.set_icon(Some(icon));
        let _ = self._tray_icon.set_tooltip(Some(tooltip));
    }

}
