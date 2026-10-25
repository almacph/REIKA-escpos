# ESC/POS Commands Specification

## Purpose

Define the tagged JSON command contract and ESC/POS byte behavior shared by
the REIKA consumer, normal desktop prints, and reprints.

Complete specification of all 42 ESC/POS commands supported by the service.
## Requirements
### Requirement: Tagged JSON Command Serialization

The system SHALL deserialize commands using Serde's internally tagged enum format.

**Implementation:** `src/models/command.rs:10-15`

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "command", content = "parameters")]
pub enum Command {
    // 42 variants...
}
```

#### Scenario: Command with parameters
- **WHEN** JSON `{"command": "Bold", "parameters": true}` is received
- **THEN** deserializes to `Command::Bold(true)`

#### Scenario: Command without parameters
- **WHEN** JSON `{"command": "Init"}` is received
- **THEN** deserializes to `Command::Init(None)` (Option<()>)

#### Scenario: Invalid command name
- **WHEN** JSON has unknown command name
- **THEN** serde returns deserialization error (HTTP 400)

---

### Requirement: Printer Control Commands

The system SHALL support printer initialization, reset, and paper control commands.

**Implementation:** `src/models/command.rs` + `src/services/printer.rs:263-302`

| Command | ESC/POS Bytes | Description |
|---------|---------------|-------------|
| `Print` | (flush buffer) | Send buffered data to printer |
| `Init` | ESC @ (0x1B 0x40) | Initialize printer to defaults |
| `Reset` | ESC ? (0x1B 0x3F) | Reset to factory settings |
| `Cut` | GS V 0 (0x1D 0x56 0x00) | Full paper cut |
| `PartialCut` | GS V 1 (0x1D 0x56 0x01) | Partial paper cut |
| `PrintCut` | GS V 65 (0x1D 0x56 0x41) | Print buffer then cut |

#### Scenario: Init resets formatting
- **WHEN** `{"command": "Init"}` is executed
- **THEN** all text formatting (bold, underline, size) returns to defaults

#### Scenario: PrintCut finalizes receipt
- **WHEN** `{"command": "PrintCut"}` is executed
- **THEN** buffer is printed and paper is cut (default end-of-receipt)

---

### Requirement: Text Formatting Commands

The system SHALL support bold, underline, double-strike, reverse, and smoothing text styles. It SHALL accept `Italic` with a boolean parameter for saved-receipt compatibility, but SHALL NOT emit italic-on or italic-off commands or apply italic preview styling. Init/Reset and reprint marker restoration SHALL NOT reactivate italic styling.

#### Scenario: Bold text enabled
- **WHEN** `{"command":"Bold","parameters":true}` is processed
- **THEN** subsequent text prints in bold until `Bold(false)`

#### Scenario: Underline modes
- **WHEN** `{"command":"Underline","parameters":"Single"}` is processed
- **THEN** subsequent text has single underline, with None and Double remaining supported

#### Scenario: Reverse mode (white on black)
- **WHEN** `{"command":"Reverse","parameters":true}` is processed
- **THEN** subsequent text prints white on black

#### Scenario: Legacy italic receipt
- **WHEN** a saved receipt contains boolean Italic commands around text
- **THEN** parsing succeeds and the text remains regular in printing and preview
- **AND** no `1B 34` or `1B 35` control commands are emitted
- **AND** other supported formatting is preserved

### Requirement: Text Size and Font Commands

The system SHALL support text size multipliers and font selection.

**Implementation:** `src/models/command.rs:47-55`

```rust
Size((u8, u8)),               // GS ! n (0x1D 0x21 n) - width x height
ResetSize(Option<()>),        // Reset to 1x1
Font(Font),                   // ESC M n (0x1B 0x4D n) - A, B, or C
```

#### Scenario: Size multiplier
- **WHEN** `{"command": "Size", "parameters": [2, 3]}`
- **THEN** text prints at 2x width and 3x height (range: 1-8 each)

#### Scenario: Reset size
- **WHEN** `{"command": "ResetSize"}`
- **THEN** text returns to 1x1 (normal) size

#### Scenario: Font selection
- **WHEN** `{"command": "Font", "parameters": "B"}`
- **THEN** subsequent text uses Font B (typically condensed)

---

### Requirement: Text Alignment Commands

The system SHALL support left, center, and right text justification.

**Implementation:** `src/models/command.rs:57-65`

```rust
Justify(JustifyMode),         // ESC a n (0x1B 0x61 n)

pub enum JustifyMode {
    LEFT,    // 0 - Left aligned (default)
    CENTER,  // 1 - Center aligned
    RIGHT,   // 2 - Right aligned
}
```

#### Scenario: Center alignment
- **WHEN** `{"command": "Justify", "parameters": "CENTER"}`
- **THEN** subsequent text is centered on 58mm paper width

#### Scenario: Right alignment
- **WHEN** `{"command": "Justify", "parameters": "RIGHT"}`
- **THEN** subsequent text is right-aligned

---

### Requirement: Text Orientation Commands

The system SHALL support text flip and upside-down printing.

**Implementation:** `src/models/command.rs:67-70`

```rust
Flip(bool),                   // ESC { n (0x1B 0x7B n)
UpsideDown(bool),             // ESC { n (0x1B 0x7B n)
```

#### Scenario: Flip enabled
- **WHEN** `{"command": "Flip", "parameters": true}`
- **THEN** text prints rotated 180 degrees

#### Scenario: Upside down printing
- **WHEN** `{"command": "UpsideDown", "parameters": true}`
- **THEN** text prints inverted vertically

---

### Requirement: Line Spacing Commands

The system SHALL support configurable line spacing and paper feeding.

**Implementation:** `src/models/command.rs:72-80`

```rust
Feed(bool),                   // Feed one line
Feeds(u8),                    // ESC d n (0x1B 0x64 n) - Feed n lines
LineSpacing(u8),              // ESC 3 n (0x1B 0x33 n) - Set spacing in dots
ResetLineSpacing(Option<()>), // Reset to default spacing
```

#### Scenario: Feed multiple lines
- **WHEN** `{"command": "Feeds", "parameters": 5}`
- **THEN** printer feeds 5 blank lines

#### Scenario: Custom line spacing
- **WHEN** `{"command": "LineSpacing", "parameters": 60}`
- **THEN** line spacing set to 60 dots (about 7.5mm at 203 DPI)

---

### Requirement: Text Output Commands

The system SHALL support text output with and without newline.

**Implementation:** `src/models/command.rs:82-85`

```rust
Write(String),                // Print text without newline
Writeln(String),              // Print text with newline (LF)
```

#### Scenario: Write without newline
- **WHEN** `{"command": "Write", "parameters": "Price: "}`
- **THEN** text prints without line feed (cursor stays on same line)

#### Scenario: Writeln with newline
- **WHEN** `{"command": "Writeln", "parameters": "Total: $100.00"}`
- **THEN** text prints followed by line feed

---

### Requirement: Character Encoding Commands

The system SHALL support 37 code pages and 29 character sets.

**Implementation:** `src/models/command.rs:87-175`

```rust
PageCode(PageCode),           // ESC t n (0x1B 0x74 n)
CharacterSet(CharacterSet),   // ESC R n (0x1B 0x52 n)

pub enum PageCode {
    PC437, Katakana, PC850, PC860, PC863, PC865, Hiragana,
    PC851, PC853, PC857, PC737, ISO8859_7, WPC1252, PC866,
    PC852, PC858, PC720, WPC775, PC855, PC861, PC862, PC864,
    PC869, ISO8859_2, ISO8859_15, PC1098, PC1118, PC1119,
    PC1125, WPC1250, WPC1251, WPC1253, WPC1254, WPC1255,
    WPC1256, WPC1257, WPC1258, KZ1048,
}

pub enum CharacterSet {
    USA, France, Germany, UK, Denmark1, Sweden, Italy, Spain1,
    Japan, Norway, Denmark2, Spain2, LatinAmerica, Korea,
    SloveniaCroatia, China, Vietnam, Arabia, IndiaDevanagari,
    IndiaBengali, IndiaTamil, IndiaTelugu, IndiaAssamese,
    IndiaOriya, IndiaKannada, IndiaMalayalam, IndiaGujarati,
    IndiaPunjabi, IndiaMarathi,
}
```

#### Scenario: Page code for multilingual
- **WHEN** `{"command": "PageCode", "parameters": "WPC1252"}`
- **THEN** printer uses Windows-1252 encoding (Western European)

#### Scenario: Japanese character set
- **WHEN** `{"command": "CharacterSet", "parameters": "Japan"}`
- **THEN** printer uses Japanese character mapping

---

### Requirement: Cash Drawer Command

The system SHALL support triggering cash drawer via RJ11 connector.

**Implementation:** `src/models/command.rs:177-185`

```rust
CashDrawer(CashDrawer),       // ESC p m t1 t2 (0x1B 0x70 m t1 t2)

pub enum CashDrawer {
    Pin2,                     // RJ11 pin 2 & 3
    Pin5,                     // RJ11 pin 5 & 4
}
```

#### Scenario: Open cash drawer pin 2
- **WHEN** `{"command": "CashDrawer", "parameters": "Pin2"}`
- **THEN** sends pulse to RJ11 connector pin 2 (most common)

#### Scenario: Open cash drawer pin 5
- **WHEN** `{"command": "CashDrawer", "parameters": "Pin5"}`
- **THEN** sends pulse to RJ11 connector pin 5 (alternate wiring)

### Requirement: Contract Extensions

The system SHALL support explicit drawer pulse timing and bounded one-bit
raster images. `CashDrawerPulse` SHALL encode `ESC p` with the selected pin
and duration, while `BitImage` SHALL use normal `GS v 0` bands of at most 24
rows. Legacy `Italic` remains parseable but has no printer or preview effect.

#### Scenario: Explicit drawer pulse
- **WHEN** `CashDrawerPulse` is given Pin2 and pulseMs 100
- **THEN** the service emits `1B 70 00 32 FF`

#### Scenario: Raster banding
- **WHEN** a valid BitImage exceeds 24 rows
- **THEN** the service emits ordered normal-mode bands and preserves row data

---

### Requirement: 1D Barcode Commands

The system SHALL support 7 barcode types for product identification.

**Implementation:** `src/models/command.rs:187-210`

```rust
Ean13(String),                // GS k 67 (13-digit European Article Number)
Ean8(String),                 // GS k 68 (8-digit EAN)
Upca(String),                 // GS k 65 (12-digit Universal Product Code)
Upce(String),                 // GS k 66 (6-digit compressed UPC)
Code39(String),               // GS k 69 (alphanumeric industrial)
Codabar(String),              // GS k 71 (library/blood bank)
Itf(String),                  // GS k 70 (Interleaved 2 of 5)
```

#### Scenario: EAN-13 barcode
- **WHEN** `{"command": "Ean13", "parameters": "4006381333931"}`
- **THEN** prints standard retail barcode (12-13 digits)

#### Scenario: Code39 alphanumeric
- **WHEN** `{"command": "Code39", "parameters": "ABC-123"}`
- **THEN** prints Code 39 barcode (supports A-Z, 0-9, special chars)

---

### Requirement: 2D Code Commands

The system SHALL support Qrcode, GS1Databar2d, Pdf417, MaxiCode, DataMatrix, and Aztec string commands. It SHALL additionally accept `Maxicode` as an alias for `MaxiCode` and SHALL retain the existing `MaxiCode` serialization spelling for desktop persistence.

#### Scenario: QR code for URL
- **WHEN** `{"command":"Qrcode","parameters":"https://receipt.example.com/123"}` is processed
- **THEN** it prints a scannable QR code linking to the digital receipt on supported hardware

#### Scenario: PDF417 for document data
- **WHEN** `{"command":"Pdf417","parameters":"Invoice data..."}` is processed
- **THEN** it prints a high-density 2D barcode on supported hardware

#### Scenario: Both MaxiCode spellings
- **WHEN** a command uses either Maxicode or MaxiCode with the same data
- **THEN** both resolve to the same printer operation
- **AND** existing desktop logs remain readable

### Requirement: Configurable Cash Drawer Pulse

The system SHALL accept `CashDrawerPulse` with parameters `pin` (Pin2 or Pin5) and `pulseMs` (100, 150, or 200). It SHALL emit `1B 70 m t1 FF`, where m is 0 for Pin2 or 1 for Pin5 and t1 is pulseMs divided by 2. Existing CashDrawer commands SHALL retain their behavior. Delivery success SHALL NOT assert physical drawer state.

#### Scenario: Supported duration and pin
- **WHEN** Pin5 is requested with pulseMs 150
- **THEN** the encoded command is `1B 70 01 4B FF`

#### Scenario: Unsupported pulse
- **WHEN** a pulse request specifies an unknown pin or duration outside 100, 150, and 200
- **THEN** the job fails input validation before any USB output

### Requirement: Bounded Raster Images

The system SHALL accept BitImage parameters containing pixel width, pixel height, and base64 row-major one-bit raster data. Width SHALL be an integer from 1 through 576 and height an integer from 1 through 4096. Decoded data SHALL contain exactly ceil(width/8) times height bytes and SHALL NOT exceed 294912 bytes. Encoding SHALL use normal-mode GS v 0 bands of at most 24 rows, in order, without adding unrelated cuts or changing text formatting.

#### Scenario: Image with partial final byte
- **WHEN** a 9-pixel-wide, 25-row image contains exactly 50 decoded bytes
- **THEN** it is encoded with two bytes per row in bands of 24 rows and 1 row
- **AND** raster bytes retain their original row order

#### Scenario: Invalid image
- **WHEN** dimensions, base64 data, decoded length, or the image size limit are invalid
- **THEN** the entire job is rejected before any USB write

## Complete Command Reference

| Category | Commands | Count |
|----------|----------|-------|
| **Control** | Print, Init, Reset, Cut, PartialCut, PrintCut | 6 |
| **Boolean Formatting** | Bold, Italic, DoubleStrike, Flip, Reverse, Smoothing, Feed, UpsideDown | 8 |
| **Numeric** | Feeds, LineSpacing | 2 |
| **String Output** | Write, Writeln | 2 |
| **Size** | Size, ResetSize | 2 |
| **Enum Formatting** | Underline, Font, Justify, PageCode, CharacterSet, CashDrawer | 6 |
| **1D Barcodes** | Ean13, Ean8, Upca, Upce, Code39, Codabar, Itf | 7 |
| **2D Codes** | Qrcode, GS1Databar2d, Pdf417, MaxiCode/Maxicode, DataMatrix, Aztec | 6 |
| **Drawer/Raster** | CashDrawerPulse, BitImage | 2 |
| **Misc** | ResetLineSpacing | 1 |
| **Total** | | **42** |

---

## JSON Request Format

```json
{
  "commands": [
    { "command": "Init" },
    { "command": "Justify", "parameters": "CENTER" },
    { "command": "Bold", "parameters": true },
    { "command": "Size", "parameters": [2, 2] },
    { "command": "Writeln", "parameters": "RECEIPT" },
    { "command": "ResetSize" },
    { "command": "Bold", "parameters": false },
    { "command": "Justify", "parameters": "LEFT" },
    { "command": "Writeln", "parameters": "Item 1          $10.00" },
    { "command": "Writeln", "parameters": "Item 2          $20.00" },
    { "command": "Writeln", "parameters": "----------------------" },
    { "command": "Bold", "parameters": true },
    { "command": "Writeln", "parameters": "Total           $30.00" },
    { "command": "Feeds", "parameters": 2 },
    { "command": "Qrcode", "parameters": "https://receipt.example.com/abc123" },
    { "command": "PrintCut" }
  ]
}
```

---

## Design Decisions

### Serde Tag Format

Using `#[serde(tag = "command", content = "parameters")]` provides:

1. **Clear JSON structure**: Command name is explicit field
2. **Optional parameters**: `Option<()>` allows omitting `parameters` key
3. **Type safety**: Enum variants enforce valid command/parameter combinations
4. **Extensibility**: New commands require only adding enum variant

### Delegation to escpos Crate

Commands delegate to `escpos` crate methods rather than generating raw bytes:

```rust
Command::Bold(enabled) => printer.bold(*enabled),
Command::Qrcode(data) => printer.qrcode(data),
```

Benefits:
- Protocol correctness handled by well-tested library
- Automatic handling of complex sequences (QR codes, barcodes)
- Support for different printer protocols
