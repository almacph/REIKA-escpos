use escpos::{driver::UsbDriver, errors::PrinterError, printer::Printer, utils::{CashDrawer, CharacterSet, Font, JustifyMode, PageCode, Protocol, UnderlineMode}};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PrinterTestSchema {
    test_page: bool,
    test_line: String,
}

impl PrinterTestSchema {
    pub fn test_line(&self) -> &str {
        &self.test_line
    }
    pub fn test_page(&self) -> &bool {
        &self.test_page
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StatusResponse {
    pub is_connected: bool,
    pub error: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "command", content = "parameters")]
pub enum Command {
    Print(Option<()>),
    Init(Option<()>),
    Reset(Option<()>),
    Cut(Option<()>),
    PartialCut(Option<()>),
    PrintCut(Option<()>),
    PageCode(PageCode),
    CharacterSet(CharacterSet),
    Bold(bool),
    Underline(UnderlineMode),
    DoubleStrike(bool),
    Font(Font),
    Flip(bool),
    Justify(JustifyMode),
    Reverse(bool),
    Size((u8, u8)),
    ResetSize(Option<()>),
    Smoothing(bool),
    Feed(bool),
    Feeds(u8),
    LineSpacing(u8),
    ResetLineSpacing(Option<()>),
    UpsideDown(bool),
    CashDrawer(CashDrawer),
    Write(String),
    Writeln(String),
    Ean13(String),
    Ean8(String),
    Upca(String),
    Upce(String),
    Code39(String),
    Codabar(String),
    Itf(String),
    Qrcode(String),
    GS1Databar2d(String),
    Pdf417(String),
    MaxiCode(String),
    DataMatrix(String),
    Aztec(String),
    // BitImage(String),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Commands {
    pub commands: Vec<Command>,
}

pub fn parse_json(json_data: &str) -> Result<Commands, PrinterError> {
    println!("Parsing a print request! {:#?}", json_data);
    let commands: Commands = serde_json::from_str(json_data).map_err(|e| PrinterError::Input(e.to_string()))?;
    println!("{:?}", commands);
    Ok(commands)
}

pub async fn execute_commands(driver: UsbDriver, commands: Commands) -> Result<(), PrinterError> {
    let mut printer = Printer::new(driver.clone(), Protocol::default(), None);
    
    printer.init()?;
    
    for command in commands.commands {
        
        match command {
            Command::Print(_) => printer.print()?,
            Command::Init(_) => printer.init()?,
            Command::Reset(_) => printer.reset()?,
            Command::Cut(_) => printer.cut()?,
            Command::PartialCut(_) => printer.partial_cut()?,
            Command::PrintCut(_) => printer.print_cut()?,
            Command::PageCode(page_code) => printer.page_code(page_code)?,
            Command::CharacterSet(char_set) => printer.character_set(char_set)?,
            Command::Bold(enabled) => printer.bold(enabled)?,
            Command::Underline(mode) => printer.underline(mode)?,
            Command::DoubleStrike(enabled) => printer.double_strike(enabled)?,
            Command::Font(font) => printer.font(font)?,
            Command::Flip(enabled) => printer.flip(enabled)?,
            Command::Justify(mode) => printer.justify(mode)?,
            Command::Reverse(enabled) => printer.reverse(enabled)?,
            Command::Size((width, height)) => printer.size(width, height)?,
            Command::ResetSize(_) => printer.reset_size()?,
            Command::Smoothing(enabled) => printer.smoothing(enabled)?,
            Command::Feed(_) => printer.feed()?,
            Command::Feeds(lines) => printer.feeds(lines)?,
            Command::LineSpacing(value) => printer.line_spacing(value)?,
            Command::ResetLineSpacing(_) => printer.reset_line_spacing()?,
            Command::UpsideDown(enabled) => printer.upside_down(enabled)?,
            Command::CashDrawer(pin) => printer.cash_drawer(pin)?,
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
            Command::Aztec(data) => printer.aztec(&data)?
            // // Command::BitImage(data) => { printer = printer.bit_image(&data)?; },
        };
    }

    printer.print_cut()?;
    Ok(())
}
