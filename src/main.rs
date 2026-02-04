#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod error;
mod handlers;
mod models;
mod routes;
mod server;
mod services;

use crate::app::{
    init_file_logging, init_noop_logging, show_already_running_dialog, AppConfig, PrinterApp,
    PrintLog, SingleInstance, SingleInstanceError, SystemTray,
};
use crate::server::run_with_port;
use crate::services::{PrinterService, UsbConfig};
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

fn main() {
    let _instance = match SingleInstance::acquire() {
        Ok(instance) => instance,
        Err(SingleInstanceError::AlreadyRunning) => {
            if show_already_running_dialog() {
                eprintln!("User chose to close existing instance - feature pending");
            }
            return;
        }
        Err(e) => {
            eprintln!("Failed to check single instance: {}", e);
            return;
        }
    };

    let config = AppConfig::load();

    // Initialize logging based on config
    if config.ui.logging_enabled {
        if let Err(e) = init_file_logging() {
            eprintln!("Failed to initialize file logging: {}", e);
        }
    } else {
        init_noop_logging();
    }
    let print_log = Arc::new(Mutex::new(PrintLog::load(config.ui.max_log_entries)));

    let (online_tx, online_rx) = watch::channel(false);

    let tray_active = SystemTray::new().is_ok();

    let server_config = config.clone();
    let server_online_tx = online_tx.clone();
    let server_print_log = print_log.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        rt.block_on(async {
            let usb_config = UsbConfig {
                vendor_id: server_config.printer.resolved_vendor_id(),
                product_id: server_config.printer.resolved_product_id(),
                endpoint: server_config.printer.resolved_endpoint(),
                interface: server_config.printer.resolved_interface(),
            };
            let port = server_config.server.port;

            loop {
                if let Some(driver) = PrinterService::try_open(&usb_config) {
                    let _ = server_online_tx.send(true);
                    let service = PrinterService::new(driver, usb_config.clone())
                        .with_status(server_online_tx.clone());
                    run_with_port(service, server_print_log.clone(), port).await;
                } else {
                    let _ = server_online_tx.send(false);
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        });
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([450.0, 500.0])
            .with_min_inner_size([400.0, 400.0]),
        ..Default::default()
    };

    let _ = eframe::run_native(
        "REIKA Printer Service",
        options,
        Box::new(move |cc| {
            Ok(Box::new(PrinterApp::new(
                cc,
                config,
                print_log,
                online_rx,
                tray_active,
            )))
        }),
    );
}
