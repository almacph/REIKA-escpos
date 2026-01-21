use std::sync::mpsc::{channel, Receiver, Sender};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder,
};

pub enum TrayMessage {
    Show,
    Exit,
    UpdateStatus(bool),
}

pub struct SystemTray {
    _tray_icon: TrayIcon,
    pub message_sender: Sender<TrayMessage>,
    message_receiver: Receiver<TrayMessage>,
    show_item_id: tray_icon::menu::MenuId,
    exit_item_id: tray_icon::menu::MenuId,
}

impl SystemTray {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let (message_sender, message_receiver) = channel();

        let show_item = MenuItem::new("Show Window", true, None);
        let exit_item = MenuItem::new("Exit", true, None);

        let show_item_id = show_item.id().clone();
        let exit_item_id = exit_item.id().clone();

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
            message_sender,
            message_receiver,
            show_item_id,
            exit_item_id,
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

    pub fn poll_events(&mut self) -> Option<TrayMessage> {
        // Check menu events from tray-icon's global receiver
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == self.show_item_id {
                return Some(TrayMessage::Show);
            } else if event.id == self.exit_item_id {
                return Some(TrayMessage::Exit);
            }
        }

        // Check internal message channel
        if let Ok(msg) = self.message_receiver.try_recv() {
            return Some(msg);
        }

        None
    }
}
