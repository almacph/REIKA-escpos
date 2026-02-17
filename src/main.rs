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
use crate::models::Command;
use crate::server::run_with_port;
use crate::services::{PrinterService, SensorReporter, UsbConfig};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
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

    // Channel for GUI → server reprint requests
    let (reprint_tx, reprint_rx) = std::sync::mpsc::channel::<Vec<Command>>();
    let reprint_rx = Arc::new(Mutex::new(reprint_rx));
    let reprint_in_progress = Arc::new(AtomicBool::new(false));
    let reprint_result: Arc<Mutex<Option<Result<(), String>>>> = Arc::new(Mutex::new(None));

    let tray_active = SystemTray::new().is_ok();

    let server_config = config.clone();
    let server_online_tx = online_tx.clone();
    let server_print_log = print_log.clone();
    let server_reprint_rx = reprint_rx.clone();
    let server_reprint_in_progress = reprint_in_progress.clone();
    let server_reprint_result = reprint_result.clone();
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

            // Set up sensor reporter (only if API key is configured)
            let (sensor_tx, sensor_rx) = tokio::sync::mpsc::channel(64);
            if let Some(reporter) = SensorReporter::new(
                server_config.reika.api_key.clone(),
                server_config.reika.server_url.clone(),
            ) {
                let sensor_online_rx = server_online_tx.subscribe();
                tokio::spawn(async move {
                    reporter.run(sensor_online_rx, sensor_rx).await;
                });
            }

            loop {
                if let Some(driver) = PrinterService::try_open(&usb_config) {
                    let driver = driver.with_sensor(sensor_tx.clone());
                    let _ = server_online_tx.send(true);
                    let service = PrinterService::new(driver, usb_config.clone())
                        .with_status(server_online_tx.clone())
                        .with_sensor(sensor_tx.clone());

                    // Spawn reprint listener task
                    let reprint_service = service.clone();
                    let reprint_rx = server_reprint_rx.clone();
                    let reprint_flag = server_reprint_in_progress.clone();
                    let reprint_res = server_reprint_result.clone();
                    tokio::spawn(async move {
                        loop {
                            let commands = {
                                let rx = reprint_rx.lock();
                                match rx {
                                    Ok(rx) => match rx.try_recv() {
                                        Ok(cmds) => Some(cmds),
                                        Err(_) => None,
                                    },
                                    Err(_) => None,
                                }
                            };

                            if let Some(cmds) = commands {
                                let result = reprint_service
                                    .execute_reprint_commands(crate::models::Commands { commands: cmds })
                                    .await;
                                if let Ok(mut res) = reprint_res.lock() {
                                    *res = Some(result.map_err(|e| e.to_string()));
                                }
                                reprint_flag.store(false, Ordering::SeqCst);
                            }

                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }
                    });

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
                reprint_tx,
                reprint_in_progress,
                reprint_result,
            )))
        }),
    );
}
