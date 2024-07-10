use std::future::Future;
use std::time::Duration;
use escpos::errors::PrinterError;
use escpos::{driver::UsbDriver, printer::Printer, utils::*};
use tokio::time::sleep;

use crate::models::{execute_commands, parse_json, Command, Commands, PrinterTestSchema};

pub async fn initialize_device() -> UsbDriver {
    loop {
        match UsbDriver::open(0x0483, 0x5840, None) {
            Ok(driver) => {
                return driver;
            },
            _ => {
                println!("Failed to open the USB driver. Retrying in 5 seconds");
                sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

async fn reconnect_device(driver: &mut UsbDriver) {
    println!("Attempting to reconnect to the USB device...");
    *driver = initialize_device().await;
    println!("Reconnected to the USB device.");
}

async fn ensure_driver<F, Fut, T>(mut driver: UsbDriver, f: F) -> Result<T, PrinterError>
where
    F: Fn(UsbDriver) -> Fut,
    Fut: Future<Output = Result<T, PrinterError>>,
{
    loop {
        let fut = f(driver.clone());
        match fut.await {
            Ok(result) => return Ok(result),
            Err(_) => {
                reconnect_device(&mut driver).await;
            }
        }
    }
}

async fn initial_attempt<F, Fut>(driver: UsbDriver, f: F) -> bool
where
    F: Fn(UsbDriver) -> Fut,
    Fut: Future<Output = Result<(), PrinterError>>,
{
    match f(driver).await {
        Ok(_) => true,
        Err(_) => false,
    }
}

async fn retry_attempt<F, Fut>(mut driver: UsbDriver, f: F) -> bool
where
    F: Fn(UsbDriver) -> Fut,
    Fut: Future<Output = Result<(), PrinterError>>,
{
    loop {
        let fut = f(driver.clone());
        match fut.await {
            Ok(_) => return true,
            Err(_) => {
                reconnect_device(&mut driver).await;
            }
        }
    }
}

pub async fn handle_test_print(
    driver: UsbDriver,
    print_request: PrinterTestSchema,
) -> Result<(), PrinterError> {
    ensure_driver(driver, move |d| {
        let print_request = print_request.clone();
        async move {
            if *print_request.test_page() {
                let test_commands = Commands {
                    commands: vec![
                        Command::Smoothing(true),
                        Command::Bold(true),
                        Command::Underline(UnderlineMode::Single),
                        Command::Writeln("Bold underline".to_string()),
                        Command::Justify(JustifyMode::CENTER),
                        Command::Reverse(true),
                        Command::Bold(false),
                        Command::Writeln("Hello world - Reverse".to_string()),
                        Command::Feed(true),
                        Command::Justify(JustifyMode::RIGHT),
                        Command::Reverse(false),
                        Command::Underline(UnderlineMode::None),
                        Command::Size((2, 3)),
                        Command::Writeln("Hello world - Normal".to_string()),
                        Command::PrintCut(vec![]),
                    ],
                };
                execute_commands(d.clone(), test_commands).await?;
            }

            if !print_request.test_line().is_empty() {
                let line_commands = Commands {
                    commands: vec![Command::Writeln(print_request.test_line().to_string()), Command::PrintCut(vec![])],
                };
                execute_commands(d, line_commands).await?;
            }
            Ok(())
        }
    }).await
}

pub async fn print_receipt(driver: UsbDriver, json_commands: &str) -> Result<(), PrinterError> {
    ensure_driver(driver, move |d| {
        let json_commands = json_commands.to_string();
        async move {
            let commands = parse_json(&json_commands)?;
            execute_commands(d, commands).await?;
            Ok(())
        }
    }).await.map_err(|e| PrinterError::Io(e.to_string())) // Manually convert to PrinterError here
}

pub async fn is_device_connected(driver: UsbDriver) -> bool {
    if !initial_attempt(driver.clone(), |d| async move {
        let mut printer = Printer::new(d.clone(), Protocol::default(), None);
        printer.init()?;
        printer.smoothing(true)?;
        printer.print_cut()?;
        Ok(())
    }).await {
        retry_attempt(driver, |d| async move {
            let mut printer = Printer::new(d.clone(), Protocol::default(), None);
            printer.init()?;
            printer.smoothing(true)?;
            printer.print_cut()?;
            Ok(())
        }).await
    } else {
        true
    }
}