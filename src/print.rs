use std::future::Future;
use std::time::Duration;
use escpos::errors::PrinterError;
use escpos::{driver::UsbDriver, printer::Printer, utils::*};
use tokio::time::sleep;
use escpos::driver::Driver;

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

pub async fn get_printer_status(driver: UsbDriver) -> Result<(), PrinterError> {
    ensure_driver(driver, move |d| async move {
        Printer::new(d.clone(), Protocol::default(), None)
            .debug_mode(Some(DebugMode::Dec))
            .real_time_status(RealTimeStatusRequest::Printer)?
            .real_time_status(RealTimeStatusRequest::RollPaperSensor)?
            .send_status()?;

        let mut buf = [0; 1];
        d.read(&mut buf)?;

        let status = RealTimeStatusResponse::parse(RealTimeStatusRequest::Printer, buf[0])?;
        println!(
            "Printer online: {}",
            status.get(&RealTimeStatusResponse::Online).unwrap_or(&false)
        );
        Ok(())
    }).await
}