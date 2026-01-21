use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use escpos::driver::UsbDriver;
use escpos::errors::PrinterError;
use escpos::printer::Printer;
use escpos::utils::Protocol;
use tokio::sync::Mutex;
use tokio::time::sleep;

use crate::error::AppError;
use crate::models::{Command, Commands, JustifyMode, PrinterTestSchema, UnderlineMode};

#[derive(Clone)]
pub struct PrinterService {
    driver: Arc<Mutex<UsbDriver>>,
}

impl PrinterService {
    pub fn new(driver: UsbDriver) -> Self {
        Self {
            driver: Arc::new(Mutex::new(driver)),
        }
    }

    pub async fn initialize_device() -> UsbDriver {
        loop {
            match UsbDriver::open(0x0483, 0x5840, None, None) {
                Ok(driver) => {
                    return driver;
                }
                _ => {
                    println!("Failed to open the USB driver. Retrying in 5 seconds");
                    sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }

    async fn reconnect(&self) {
        println!("Attempting to reconnect to the USB device...");
        let new_driver = Self::initialize_device().await;
        let mut driver = self.driver.lock().await;
        *driver = new_driver;
        println!("Reconnected to the USB device.");
    }

    async fn with_retry<F, Fut, T>(&self, f: F) -> Result<T, AppError>
    where
        F: Fn(UsbDriver) -> Fut,
        Fut: Future<Output = Result<T, PrinterError>>,
    {
        loop {
            let driver = self.driver.lock().await.clone();
            match f(driver).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    println!("Operation failed: {:?}, attempting reconnect...", e);
                    self.reconnect().await;
                }
            }
        }
    }

    pub async fn check_connection(&self) -> bool {
        let driver = self.driver.lock().await.clone();

        let initial_result = Self::try_init(driver).await;
        if initial_result.is_ok() {
            return true;
        }

        self.reconnect().await;
        let driver = self.driver.lock().await.clone();
        Self::try_init(driver).await.is_ok()
    }

    async fn try_init(driver: UsbDriver) -> Result<(), PrinterError> {
        let mut printer = Printer::new(driver, Protocol::default(), None);
        printer.init()?;
        printer.smoothing(true)?;
        printer.print_cut()?;
        Ok(())
    }

    pub async fn execute_commands(&self, commands: Commands) -> Result<(), AppError> {
        self.with_retry(|driver| {
            let commands_clone = commands.commands.clone();
            async move { Self::execute_commands_inner(driver, commands_clone).await }
        })
        .await
    }

    async fn execute_commands_inner(
        driver: UsbDriver,
        commands: Vec<Command>,
    ) -> Result<(), PrinterError> {
        let mut printer = Printer::new(driver, Protocol::default(), None);
        printer.init()?;

        for command in commands {
            match command {
                Command::Print(_) => printer.print()?,
                Command::Init(_) => printer.init()?,
                Command::Reset(_) => printer.reset()?,
                Command::Cut(_) => printer.cut()?,
                Command::PartialCut(_) => printer.partial_cut()?,
                Command::PrintCut(_) => printer.print_cut()?,
                Command::PageCode(page_code) => printer.page_code(page_code.into())?,
                Command::CharacterSet(char_set) => printer.character_set(char_set.into())?,
                Command::Bold(enabled) => printer.bold(enabled)?,
                Command::Underline(mode) => printer.underline(mode.into())?,
                Command::DoubleStrike(enabled) => printer.double_strike(enabled)?,
                Command::Font(font) => printer.font(font.into())?,
                Command::Flip(enabled) => printer.flip(enabled)?,
                Command::Justify(mode) => printer.justify(mode.into())?,
                Command::Reverse(enabled) => printer.reverse(enabled)?,
                Command::Size((width, height)) => printer.size(width, height)?,
                Command::ResetSize(_) => printer.reset_size()?,
                Command::Smoothing(enabled) => printer.smoothing(enabled)?,
                Command::Feed(_) => printer.feed()?,
                Command::Feeds(lines) => printer.feeds(lines)?,
                Command::LineSpacing(value) => printer.line_spacing(value)?,
                Command::ResetLineSpacing(_) => printer.reset_line_spacing()?,
                Command::UpsideDown(enabled) => printer.upside_down(enabled)?,
                Command::CashDrawer(pin) => printer.cash_drawer(pin.into())?,
                Command::Write(text) => printer.write(&text)?,
                Command::Writeln(text) => printer.writeln(&text)?,
                Command::Ean13(data) => printer.ean13(&data)?,
                Command::Ean8(data) => printer.ean8(&data)?,
                Command::Upca(data) => printer.upca(&data)?,
                Command::Upce(data) => printer.upce(&data)?,
                Command::Code39(data) => printer.code39(&data)?,
                Command::Codabar(data) => printer.codabar(&data)?,
                Command::Itf(data) => printer.itf(&data)?,
                Command::Qrcode(data) => printer.qrcode(&data)?,
                Command::GS1Databar2d(data) => printer.gs1_databar_2d(&data)?,
                Command::Pdf417(data) => printer.pdf417(&data)?,
                Command::MaxiCode(data) => printer.maxi_code(&data)?,
                Command::DataMatrix(data) => printer.data_matrix(&data)?,
                Command::Aztec(data) => printer.aztec(&data)?,
            };
        }

        printer.print_cut()?;
        Ok(())
    }

    pub async fn print_test(&self, request: PrinterTestSchema) -> Result<(), AppError> {
        self.with_retry(|driver| {
            let request_clone = request.clone();
            async move { Self::print_test_inner(driver, request_clone).await }
        })
        .await
    }

    async fn print_test_inner(
        driver: UsbDriver,
        request: PrinterTestSchema,
    ) -> Result<(), PrinterError> {
        if request.test_page() {
            let test_commands = vec![
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
                Command::PrintCut(None),
            ];
            Self::execute_commands_inner(driver.clone(), test_commands).await?;
        }

        if !request.test_line().is_empty() {
            let line_commands = vec![
                Command::Writeln(request.test_line().to_string()),
                Command::PrintCut(None),
            ];
            Self::execute_commands_inner(driver, line_commands).await?;
        }

        Ok(())
    }
}
