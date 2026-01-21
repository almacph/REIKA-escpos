use escpos::utils::{
    CashDrawer as EscCashDrawer, CharacterSet as EscCharacterSet, Font as EscFont,
    JustifyMode as EscJustifyMode, PageCode as EscPageCode, UnderlineMode as EscUnderlineMode,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum PageCode {
    PC437,
    Katakana,
    PC850,
    PC860,
    PC863,
    PC865,
    Hiragana,
    PC851,
    PC853,
    PC857,
    PC737,
    ISO8859_7,
    WPC1252,
    PC866,
    PC852,
    PC858,
    PC720,
    WPC775,
    PC855,
    PC861,
    PC862,
    PC864,
    PC869,
    ISO8859_2,
    ISO8859_15,
    PC1098,
    PC1118,
    PC1119,
    PC1125,
    WPC1250,
    WPC1251,
    WPC1253,
    WPC1254,
    WPC1255,
    WPC1256,
    WPC1257,
    WPC1258,
    KZ1048,
}

impl From<PageCode> for EscPageCode {
    fn from(p: PageCode) -> Self {
        match p {
            PageCode::PC437 => EscPageCode::PC437,
            PageCode::Katakana => EscPageCode::Katakana,
            PageCode::PC850 => EscPageCode::PC850,
            PageCode::PC860 => EscPageCode::PC860,
            PageCode::PC863 => EscPageCode::PC863,
            PageCode::PC865 => EscPageCode::PC865,
            PageCode::Hiragana => EscPageCode::Hiragana,
            PageCode::PC851 => EscPageCode::PC851,
            PageCode::PC853 => EscPageCode::PC853,
            PageCode::PC857 => EscPageCode::PC857,
            PageCode::PC737 => EscPageCode::PC737,
            PageCode::ISO8859_7 => EscPageCode::ISO8859_7,
            PageCode::WPC1252 => EscPageCode::WPC1252,
            PageCode::PC866 => EscPageCode::PC866,
            PageCode::PC852 => EscPageCode::PC852,
            PageCode::PC858 => EscPageCode::PC858,
            PageCode::PC720 => EscPageCode::PC720,
            PageCode::WPC775 => EscPageCode::WPC775,
            PageCode::PC855 => EscPageCode::PC855,
            PageCode::PC861 => EscPageCode::PC861,
            PageCode::PC862 => EscPageCode::PC862,
            PageCode::PC864 => EscPageCode::PC864,
            PageCode::PC869 => EscPageCode::PC869,
            PageCode::ISO8859_2 => EscPageCode::ISO8859_2,
            PageCode::ISO8859_15 => EscPageCode::ISO8859_15,
            PageCode::PC1098 => EscPageCode::PC1098,
            PageCode::PC1118 => EscPageCode::PC1118,
            PageCode::PC1119 => EscPageCode::PC1119,
            PageCode::PC1125 => EscPageCode::PC1125,
            PageCode::WPC1250 => EscPageCode::WPC1250,
            PageCode::WPC1251 => EscPageCode::WPC1251,
            PageCode::WPC1253 => EscPageCode::WPC1253,
            PageCode::WPC1254 => EscPageCode::WPC1254,
            PageCode::WPC1255 => EscPageCode::WPC1255,
            PageCode::WPC1256 => EscPageCode::WPC1256,
            PageCode::WPC1257 => EscPageCode::WPC1257,
            PageCode::WPC1258 => EscPageCode::WPC1258,
            PageCode::KZ1048 => EscPageCode::KZ1048,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum CharacterSet {
    USA,
    France,
    Germany,
    UK,
    Denmark1,
    Sweden,
    Italy,
    Spain1,
    Japan,
    Norway,
    Denmark2,
    Spain2,
    LatinAmerica,
    Korea,
    SloveniaCroatia,
    China,
    Vietnam,
    Arabia,
    IndiaDevanagari,
    IndiaBengali,
    IndiaTamil,
    IndiaTelugu,
    IndiaAssamese,
    IndiaOriya,
    IndiaKannada,
    IndiaMalayalam,
    IndiaGujarati,
    IndiaPunjabi,
    IndiaMarathi,
}

impl From<CharacterSet> for EscCharacterSet {
    fn from(c: CharacterSet) -> Self {
        match c {
            CharacterSet::USA => EscCharacterSet::USA,
            CharacterSet::France => EscCharacterSet::France,
            CharacterSet::Germany => EscCharacterSet::Germany,
            CharacterSet::UK => EscCharacterSet::UK,
            CharacterSet::Denmark1 => EscCharacterSet::Denmark1,
            CharacterSet::Sweden => EscCharacterSet::Sweden,
            CharacterSet::Italy => EscCharacterSet::Italy,
            CharacterSet::Spain1 => EscCharacterSet::Spain1,
            CharacterSet::Japan => EscCharacterSet::Japan,
            CharacterSet::Norway => EscCharacterSet::Norway,
            CharacterSet::Denmark2 => EscCharacterSet::Denmark2,
            CharacterSet::Spain2 => EscCharacterSet::Spain2,
            CharacterSet::LatinAmerica => EscCharacterSet::LatinAmerica,
            CharacterSet::Korea => EscCharacterSet::Korea,
            CharacterSet::SloveniaCroatia => EscCharacterSet::SloveniaCroatia,
            CharacterSet::China => EscCharacterSet::China,
            CharacterSet::Vietnam => EscCharacterSet::Vietnam,
            CharacterSet::Arabia => EscCharacterSet::Arabia,
            CharacterSet::IndiaDevanagari => EscCharacterSet::IndiaDevanagari,
            CharacterSet::IndiaBengali => EscCharacterSet::IndiaBengali,
            CharacterSet::IndiaTamil => EscCharacterSet::IndiaTamil,
            CharacterSet::IndiaTelugu => EscCharacterSet::IndiaTelugu,
            CharacterSet::IndiaAssamese => EscCharacterSet::IndiaAssamese,
            CharacterSet::IndiaOriya => EscCharacterSet::IndiaOriya,
            CharacterSet::IndiaKannada => EscCharacterSet::IndiaKannada,
            CharacterSet::IndiaMalayalam => EscCharacterSet::IndiaMalayalam,
            CharacterSet::IndiaGujarati => EscCharacterSet::IndiaGujarati,
            CharacterSet::IndiaPunjabi => EscCharacterSet::IndiaPunjabi,
            CharacterSet::IndiaMarathi => EscCharacterSet::IndiaMarathi,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum UnderlineMode {
    None,
    Single,
    Double,
}

impl From<UnderlineMode> for EscUnderlineMode {
    fn from(u: UnderlineMode) -> Self {
        match u {
            UnderlineMode::None => EscUnderlineMode::None,
            UnderlineMode::Single => EscUnderlineMode::Single,
            UnderlineMode::Double => EscUnderlineMode::Double,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Font {
    A,
    B,
    C,
}

impl From<Font> for EscFont {
    fn from(f: Font) -> Self {
        match f {
            Font::A => EscFont::A,
            Font::B => EscFont::B,
            Font::C => EscFont::C,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum JustifyMode {
    LEFT,
    CENTER,
    RIGHT,
}

impl From<JustifyMode> for EscJustifyMode {
    fn from(j: JustifyMode) -> Self {
        match j {
            JustifyMode::LEFT => EscJustifyMode::LEFT,
            JustifyMode::CENTER => EscJustifyMode::CENTER,
            JustifyMode::RIGHT => EscJustifyMode::RIGHT,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum CashDrawer {
    Pin2,
    Pin5,
}

impl From<CashDrawer> for EscCashDrawer {
    fn from(c: CashDrawer) -> Self {
        match c {
            CashDrawer::Pin2 => EscCashDrawer::Pin2,
            CashDrawer::Pin5 => EscCashDrawer::Pin5,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
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
}
