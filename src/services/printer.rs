use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use escpos::errors::PrinterError;
use escpos::printer::Printer;
use escpos::utils::Protocol;
use tokio::sync::watch;
use tokio::sync::Mutex;
use tokio::time::sleep;

use crate::error::AppError;
use crate::models::{Command, Commands, JustifyMode, PrinterTestSchema, UnderlineMode};
use super::reprint::inject_reprint_markers;
use super::sensor_reporter::SensorEvent;
use super::usb_driver::{CustomUsbDriver, UsbConfig};

// Global counter for generating unique print IDs
static PRINT_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Generate a unique print ID for tracking operations
fn generate_print_id() -> String {
    let counter = PRINT_COUNTER.fetch_add(1, Ordering::SeqCst);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u32;
    format!("{:04x}{:04x}", timestamp & 0xFFFF, counter & 0xFFFF)
}

#[derive(Clone)]
pub struct PrinterService {
    driver: Arc<Mutex<CustomUsbDriver>>,
    usb_config: UsbConfig,
    status_tx: Option<watch::Sender<bool>>,
    sensor_tx: Option<tokio::sync::mpsc::Sender<SensorEvent>>,
}

impl PrinterService {
    pub fn new(driver: CustomUsbDriver, usb_config: UsbConfig) -> Self {
        Self {
            driver: Arc::new(Mutex::new(driver)),
            usb_config,
            status_tx: None,
            sensor_tx: None,
        }
    }

    pub fn with_status(mut self, status_tx: watch::Sender<bool>) -> Self {
        self.status_tx = Some(status_tx);
        self
    }

    pub fn with_sensor(mut self, sensor_tx: tokio::sync::mpsc::Sender<SensorEvent>) -> Self {
        self.sensor_tx = Some(sensor_tx);
        self
    }

    fn send_sensor_event(&self, event: SensorEvent) {
        if let Some(tx) = &self.sensor_tx {
            let _ = tx.try_send(event);
        }
    }

    fn update_status(&self, online: bool) {
        if let Some(tx) = &self.status_tx {
            let _ = tx.send(online);
        }
    }

    pub async fn initialize_device_with_config(config: &UsbConfig) -> CustomUsbDriver {
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            log::info!(
                "USB init attempt #{}: VID=0x{:04X}, PID=0x{:04X}",
                attempt,
                config.vendor_id,
                config.product_id
            );
            match CustomUsbDriver::open(config) {
                Ok(driver) => {
                    log::info!("USB device opened successfully on attempt #{}", attempt);
                    return driver;
                }
                Err(e) => {
                    log::warn!(
                        "USB init attempt #{} failed: {:?}. Retrying in 5 seconds...",
                        attempt,
                        e
                    );
                    sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }

    pub fn try_open(config: &UsbConfig) -> Option<CustomUsbDriver> {
        log::debug!(
            "try_open: VID=0x{:04X}, PID=0x{:04X}, EP={:?}, IF={:?}",
            config.vendor_id,
            config.product_id,
            config.endpoint,
            config.interface
        );
        match CustomUsbDriver::open(config) {
            Ok(driver) => {
                log::info!("try_open: USB device opened successfully");
                Some(driver)
            }
            Err(e) => {
                log::debug!("try_open: USB open failed: {:?}", e);
                None
            }
        }
    }

    async fn reconnect(&self) {
        let start = Instant::now();
        self.update_status(false);
        log::info!("reconnect: Starting USB reconnection...");
        let mut new_driver = Self::initialize_device_with_config(&self.usb_config).await;
        if let Some(tx) = &self.sensor_tx {
            new_driver = new_driver.with_sensor(tx.clone());
        }
        let mut driver = self.driver.lock().await;
        *driver = new_driver;
        self.update_status(true);
        log::info!("reconnect: USB reconnected in {:?}", start.elapsed());
    }
    async fn with_retry<F, Fut, T>(&self, f: F) -> Result<T, AppError>
    where
        F: Fn(CustomUsbDriver, String) -> Fut,
        Fut: Future<Output = Result<T, PrinterError>>,
    {
        let start = Instant::now();
        let mut attempt = 0u32;
        let print_id = generate_print_id();

        // Try with existing connection first - don't refresh unless needed
        // This avoids race conditions from constantly closing/reopening USB
        log::info!("[print_id={}] Starting print operation...", print_id);

        loop {
            attempt += 1;
            let op_start = Instant::now();
            log::info!("[print_id={}] Attempt #{} starting...", print_id, attempt);

            let driver = self.driver.lock().await.clone();
            match f(driver, print_id.clone()).await {
                Ok(result) => {
                    log::info!(
                        "[PRINT_SUMMARY] print_id={} | status=OK | attempts={} | duration={:?}",
                        print_id,
                        attempt,
                        start.elapsed()
                    );
                    return Ok(result);
                }
                Err(e) => {
                    log::error!(
                        "[print_id={}] Attempt #{} FAILED after {:?}: {:?}",
                        print_id,
                        attempt,
                        op_start.elapsed(),
                        e
                    );
                    self.send_sensor_event(SensorEvent::PrintFail(
                        format!("print_id={} attempt={} error={:?}", print_id, attempt, e)
                    ));
                    // Only reconnect after failure
                    log::info!("[print_id={}] Reconnecting before retry...", print_id);
                    self.reconnect().await;
                }
            }
        }
    }

    pub async fn check_connection(&self) -> bool {
        // Try to use the existing connection without closing/reopening
        // This avoids interfering with ongoing print operations
        log::debug!("check_connection: Health check starting...");

        let driver = self.driver.lock().await.clone();
        match Self::try_init(driver).await {
            Ok(()) => {
                self.update_status(true);
                log::debug!("check_connection: Health check result = true");
                true
            }
            Err(e) => {
                log::debug!("check_connection: Health check failed: {:?}", e);
                self.update_status(false);
                log::debug!("check_connection: Health check result = false");
                false
            }
        }
    }

    async fn try_init(driver: CustomUsbDriver) -> Result<(), PrinterError> {
        log::debug!("try_init: Sending printer init command...");
        let start = Instant::now();
        let mut printer = Printer::new(driver, Protocol::default(), None);
        match printer.init() {
            Ok(_) => {
                log::debug!("try_init: Printer init OK in {:?}", start.elapsed());
                Ok(())
            }
            Err(e) => {
                log::warn!("try_init: Printer init FAILED in {:?}: {:?}", start.elapsed(), e);
                Err(e)
            }
        }
    }

    pub async fn execute_commands(&self, commands: Commands) -> Result<(), AppError> {
        self.with_retry(|driver, print_id| {
            let commands_clone = commands.commands.clone();
            async move { Self::execute_commands_inner(driver, commands_clone, print_id).await }
        })
        .await
    }

    /// Execute a reprint: injects anti-fraud markers at top, middle, and bottom,
    /// then prints with retry. Does NOT log to print log (not a new transaction).
    pub async fn execute_reprint_commands(&self, commands: Commands) -> Result<(), AppError> {
        let marked_commands = inject_reprint_markers(commands.commands);
        self.with_retry(|driver, print_id| {
            let cmds = marked_commands.clone();
            async move { Self::execute_reprint_inner(driver, cmds, print_id).await }
        })
        .await
    }

    async fn execute_reprint_inner(
        driver: CustomUsbDriver,
        commands: Vec<Command>,
        print_id: String,
    ) -> Result<(), PrinterError> {
        let start = Instant::now();
        let cmd_count = commands.len();
        log::info!(
            "[print_id={}] execute_reprint: Starting {} commands (with markers)...",
            print_id,
            cmd_count
        );

        let mut printer = Printer::new(driver, Protocol::default(), None);

        // The injected command stream already starts with Init, so we iterate directly
        for (idx, command) in commands.iter().enumerate() {
            let cmd_start = Instant::now();
            let result = match command {
                Command::Print(_) => printer.print(),
                Command::Init(_) => printer.init(),
                Command::Reset(_) => printer.reset(),
                Command::Cut(_) => printer.cut(),
                Command::PartialCut(_) => printer.partial_cut(),
                Command::PrintCut(_) => printer.print_cut(),
                Command::PageCode(page_code) => printer.page_code(page_code.clone().into()),
                Command::CharacterSet(char_set) => printer.character_set(char_set.clone().into()),
                Command::Bold(enabled) => printer.bold(*enabled),
                Command::Underline(mode) => printer.underline(mode.clone().into()),
                Command::DoubleStrike(enabled) => printer.double_strike(*enabled),
                Command::Font(font) => printer.font(font.clone().into()),
                Command::Flip(enabled) => printer.flip(*enabled),
                Command::Justify(mode) => printer.justify(mode.clone().into()),
                Command::Reverse(enabled) => printer.reverse(*enabled),
                Command::Size((width, height)) => printer.size(*width, *height),
                Command::ResetSize(_) => printer.reset_size(),
                Command::Smoothing(enabled) => printer.smoothing(*enabled),
                Command::Feed(_) => printer.feed(),
                Command::Feeds(lines) => printer.feeds(*lines),
                Command::LineSpacing(value) => printer.line_spacing(*value),
                Command::ResetLineSpacing(_) => printer.reset_line_spacing(),
                Command::UpsideDown(enabled) => printer.upside_down(*enabled),
                Command::CashDrawer(pin) => printer.cash_drawer(pin.clone().into()),
                Command::Write(text) => printer.write(text),
                Command::Writeln(text) => printer.writeln(text),
                Command::Ean13(data) => printer.ean13(data),
                Command::Ean8(data) => printer.ean8(data),
                Command::Upca(data) => printer.upca(data),
                Command::Upce(data) => printer.upce(data),
                Command::Code39(data) => printer.code39(data),
                Command::Codabar(data) => printer.codabar(data),
                Command::Itf(data) => printer.itf(data),
                Command::Qrcode(data) => printer.qrcode(data),
                Command::GS1Databar2d(data) => printer.gs1_databar_2d(data),
                Command::Pdf417(data) => printer.pdf417(data),
                Command::MaxiCode(data) => printer.maxi_code(data),
                Command::DataMatrix(data) => printer.data_matrix(data),
                Command::Aztec(data) => printer.aztec(data),
            };

            match result {
                Ok(_) => {
                    log::debug!(
                        "execute_reprint: [{}/{}] OK in {:?}",
                        idx + 1,
                        cmd_count,
                        cmd_start.elapsed()
                    );
                }
                Err(e) => {
                    log::error!(
                        "execute_reprint: [{}/{}] FAILED after {:?}: {:?}",
                        idx + 1,
                        cmd_count,
                        cmd_start.elapsed(),
                        e
                    );
                    return Err(e);
                }
            }
        }

        log::info!(
            "[print_id={}] execute_reprint: COMPLETE - {} commands in {:?}",
            print_id,
            cmd_count,
            start.elapsed()
        );
        Ok(())
    }

    async fn execute_commands_inner(
        driver: CustomUsbDriver,
        commands: Vec<Command>,
        print_id: String,
    ) -> Result<(), PrinterError> {
        let start = Instant::now();
        let cmd_count = commands.len();
        log::info!("[print_id={}] execute_commands: Starting {} commands...", print_id, cmd_count);

        let mut printer = Printer::new(driver, Protocol::default(), None);

        log::debug!("execute_commands: Sending init...");
        printer.init()?;
        log::debug!("execute_commands: Init OK");

        for (idx, command) in commands.iter().enumerate() {
            let cmd_start = Instant::now();
            let cmd_name = match command {
                Command::Print(_) => "Print",
                Command::Init(_) => "Init",
                Command::Reset(_) => "Reset",
                Command::Cut(_) => "Cut",
                Command::PartialCut(_) => "PartialCut",
                Command::PrintCut(_) => "PrintCut",
                Command::PageCode(_) => "PageCode",
                Command::CharacterSet(_) => "CharacterSet",
                Command::Bold(_) => "Bold",
                Command::Underline(_) => "Underline",
                Command::DoubleStrike(_) => "DoubleStrike",
                Command::Font(_) => "Font",
                Command::Flip(_) => "Flip",
                Command::Justify(_) => "Justify",
                Command::Reverse(_) => "Reverse",
                Command::Size(_) => "Size",
                Command::ResetSize(_) => "ResetSize",
                Command::Smoothing(_) => "Smoothing",
                Command::Feed(_) => "Feed",
                Command::Feeds(_) => "Feeds",
                Command::LineSpacing(_) => "LineSpacing",
                Command::ResetLineSpacing(_) => "ResetLineSpacing",
                Command::UpsideDown(_) => "UpsideDown",
                Command::CashDrawer(_) => "CashDrawer",
                Command::Write(_) => "Write",
                Command::Writeln(_) => "Writeln",
                Command::Ean13(_) => "Ean13",
                Command::Ean8(_) => "Ean8",
                Command::Upca(_) => "Upca",
                Command::Upce(_) => "Upce",
                Command::Code39(_) => "Code39",
                Command::Codabar(_) => "Codabar",
                Command::Itf(_) => "Itf",
                Command::Qrcode(_) => "Qrcode",
                Command::GS1Databar2d(_) => "GS1Databar2d",
                Command::Pdf417(_) => "Pdf417",
                Command::MaxiCode(_) => "MaxiCode",
                Command::DataMatrix(_) => "DataMatrix",
                Command::Aztec(_) => "Aztec",
            };

            let result = match command {
                Command::Print(_) => printer.print(),
                Command::Init(_) => printer.init(),
                Command::Reset(_) => printer.reset(),
                Command::Cut(_) => printer.cut(),
                Command::PartialCut(_) => printer.partial_cut(),
                Command::PrintCut(_) => printer.print_cut(),
                Command::PageCode(page_code) => printer.page_code(page_code.clone().into()),
                Command::CharacterSet(char_set) => printer.character_set(char_set.clone().into()),
                Command::Bold(enabled) => printer.bold(*enabled),
                Command::Underline(mode) => printer.underline(mode.clone().into()),
                Command::DoubleStrike(enabled) => printer.double_strike(*enabled),
                Command::Font(font) => printer.font(font.clone().into()),
                Command::Flip(enabled) => printer.flip(*enabled),
                Command::Justify(mode) => printer.justify(mode.clone().into()),
                Command::Reverse(enabled) => printer.reverse(*enabled),
                Command::Size((width, height)) => printer.size(*width, *height),
                Command::ResetSize(_) => printer.reset_size(),
                Command::Smoothing(enabled) => printer.smoothing(*enabled),
                Command::Feed(_) => printer.feed(),
                Command::Feeds(lines) => printer.feeds(*lines),
                Command::LineSpacing(value) => printer.line_spacing(*value),
                Command::ResetLineSpacing(_) => printer.reset_line_spacing(),
                Command::UpsideDown(enabled) => printer.upside_down(*enabled),
                Command::CashDrawer(pin) => printer.cash_drawer(pin.clone().into()),
                Command::Write(text) => printer.write(text),
                Command::Writeln(text) => printer.writeln(text),
                Command::Ean13(data) => printer.ean13(data),
                Command::Ean8(data) => printer.ean8(data),
                Command::Upca(data) => printer.upca(data),
                Command::Upce(data) => printer.upce(data),
                Command::Code39(data) => printer.code39(data),
                Command::Codabar(data) => printer.codabar(data),
                Command::Itf(data) => printer.itf(data),
                Command::Qrcode(data) => printer.qrcode(data),
                Command::GS1Databar2d(data) => printer.gs1_databar_2d(data),
                Command::Pdf417(data) => printer.pdf417(data),
                Command::MaxiCode(data) => printer.maxi_code(data),
                Command::DataMatrix(data) => printer.data_matrix(data),
                Command::Aztec(data) => printer.aztec(data),
            };

            match result {
                Ok(_) => {
                    log::debug!(
                        "execute_commands: [{}/{}] {} OK in {:?}",
                        idx + 1,
                        cmd_count,
                        cmd_name,
                        cmd_start.elapsed()
                    );
                }
                Err(e) => {
                    log::error!(
                        "execute_commands: [{}/{}] {} FAILED after {:?}: {:?}",
                        idx + 1,
                        cmd_count,
                        cmd_name,
                        cmd_start.elapsed(),
                        e
                    );
                    return Err(e);
                }
            }
        }

        log::debug!("[print_id={}] execute_commands: Sending final print_cut...", print_id);
        let cut_start = Instant::now();
        printer.print_cut()?;
        log::debug!("[print_id={}] execute_commands: print_cut OK in {:?}", print_id, cut_start.elapsed());

        log::info!(
            "[print_id={}] execute_commands: COMPLETE - {} commands in {:?}",
            print_id,
            cmd_count,
            start.elapsed()
        );
        Ok(())
    }

    pub async fn print_test(&self, request: PrinterTestSchema) -> Result<(), AppError> {
        self.with_retry(|driver, print_id| {
            let request_clone = request.clone();
            async move { Self::print_test_inner(driver, request_clone, print_id).await }
        })
        .await
    }

    async fn print_test_inner(
        driver: CustomUsbDriver,
        request: PrinterTestSchema,
        print_id: String,
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
            Self::execute_commands_inner(driver.clone(), test_commands, print_id.clone()).await?;
        }

        if !request.test_line().is_empty() {
            let line_commands = vec![
                Command::Writeln(request.test_line().to_string()),
                Command::PrintCut(None),
            ];
            Self::execute_commands_inner(driver, line_commands, print_id).await?;
        }

        Ok(())
    }
}
