use std::time::Duration;
use escpos::errors::PrinterError;
use escpos::{driver::UsbDriver, printer::Printer, utils::*};
use tokio::time::sleep;



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

pub fn handle_test_print(driver: UsbDriver) -> Result<(), PrinterError> {
    Printer::new(driver.clone(), Protocol::default(), None)
        .init()?
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

    Ok(())
}