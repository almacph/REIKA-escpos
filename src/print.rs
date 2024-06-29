use std::future::Future;
use std::time::Duration;
use escpos::errors::PrinterError;
use escpos::{driver::UsbDriver, printer::Printer, utils::*};
use tokio::time::sleep;

use crate::models::PrinterTestSchema;

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
            let mut binding = Printer::new(d.clone(), Protocol::default(), None);
            let printer = binding.init()?;

            if *print_request.test_page() {
                printer
                    .smoothing(true)?
                    .bold(true)?
                    .underline(UnderlineMode::Single)?
                    .writeln("Bold underline")?
                    .justify(JustifyMode::CENTER)?
                    .reverse(true)?
                    .bold(false)?
                    .writeln("Hello world - Reverse")?
                    .feed()?
                    .justify(JustifyMode::RIGHT)?
                    .reverse(false)?
                    .underline(UnderlineMode::None)?
                    .size(2, 3)?
                    .writeln("Hello world - Normal")?
                    .print_cut()?;
            }

            if !print_request.test_line().is_empty() {
                printer.writeln(&print_request.test_line())?.print_cut()?;
            }
            Ok(())
        }
    }).await
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