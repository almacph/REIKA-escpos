# REIKA-escpos API Specification

ESC/POS thermal printer service for the REIKA POS system.

## Consumer Application

| Field | Value |
|-------|-------|
| **Application** | REIKA (Real-time Efficient Inventory and Knowledge Administration) |
| **Repository** | https://github.com/almacph/REIKA |
| **Framework** | SvelteKit 5 (TypeScript) |
| **Schema Library** | Zod (TypeScript runtime validation) |
| **Integration Point** | Browser → HTTP → localhost:55000 |

## Service Configuration

| Setting | Value |
|---------|-------|
| **Port** | `55000` |
| **Protocol** | HTTP (localhost only) |
| **Content-Type** | `application/json` |
| **CORS** | Allow `*` (service runs on localhost) |

---

## API Endpoints

### GET `/print/test`

Check printer connection status.

**Response:**
```json
{
  "is_connected": true,
  "error": null
}
```

**Response Schema:**
```typescript
{
  is_connected: boolean,
  error?: string        // Present only when is_connected is false
}
```

---

### POST `/print/test`

Send a test print line with optional test page.

**Request:**
```json
{
  "test_line": "Hello World",
  "test_page": false
}
```

**Request Schema:**
```typescript
{
  test_line: string,    // Text to print
  test_page: boolean    // If true, print a full test page with all features
}
```

**Response:**
```json
{
  "is_connected": true,
  "error": null
}
```

---

### POST `/print`

Send ESC/POS commands to the printer.

**Request:**
```json
{
  "commands": [
    { "command": "Init" },
    { "command": "Bold", "parameters": true },
    { "command": "Writeln", "parameters": "Hello World" },
    { "command": "PrintCut" }
  ]
}
```

**Request Schema:**
```typescript
{
  commands: Command[]   // Array of command objects
}
```

**Response:**
```json
{
  "is_connected": true,
  "error": null
}
```

---

## Command Reference

### Command Object Structure

Each command is a JSON object with:
- `command`: String literal identifying the command (required)
- `parameters`: Command-specific value (required for some commands)

```typescript
{ "command": "CommandName" }
// or
{ "command": "CommandName", "parameters": <value> }
```

---

### Commands Without Parameters

| Command | ESC/POS Action |
|---------|----------------|
| `Print` | Send buffer to printer |
| `Init` | Initialize printer (ESC @) |
| `Reset` | Reset to default settings |
| `Cut` | Full paper cut |
| `PartialCut` | Partial paper cut |
| `PrintCut` | Print buffer then cut |
| `ResetSize` | Reset text size to default |
| `ResetLineSpacing` | Reset line spacing to default |

**Example:**
```json
{ "command": "Init" }
{ "command": "PrintCut" }
```

---

### Commands with Boolean Parameters

| Command | Parameter | Description |
|---------|-----------|-------------|
| `Bold` | `boolean` | Enable/disable bold text |
| `DoubleStrike` | `boolean` | Enable/disable double strike |
| `Flip` | `boolean` | Enable/disable 180° rotation |
| `Reverse` | `boolean` | Enable/disable white-on-black |
| `Smoothing` | `boolean` | Enable/disable text smoothing |
| `Feed` | `boolean` | Feed one line (true) or not |
| `UpsideDown` | `boolean` | Enable/disable upside-down printing |

**Example:**
```json
{ "command": "Bold", "parameters": true }
{ "command": "Reverse", "parameters": false }
```

---

### Commands with Number Parameters

| Command | Parameter | Description |
|---------|-----------|-------------|
| `Feeds` | `number` | Feed N lines |
| `LineSpacing` | `number` | Set line spacing in dots |

**Example:**
```json
{ "command": "Feeds", "parameters": 3 }
{ "command": "LineSpacing", "parameters": 60 }
```

---

### Commands with String Parameters

| Command | Parameter | Description |
|---------|-----------|-------------|
| `Write` | `string` | Print text (no newline) |
| `Writeln` | `string` | Print text with newline |

**Example:**
```json
{ "command": "Write", "parameters": "Price: " }
{ "command": "Writeln", "parameters": "P100.00" }
```

---

### Commands with Tuple Parameters

| Command | Parameter | Description |
|---------|-----------|-------------|
| `Size` | `[number, number]` | Set text size [width, height] multiplier (1-8) |

**Example:**
```json
{ "command": "Size", "parameters": [2, 2] }
```

---

### Barcode Commands (String Parameter)

All barcode commands take a string parameter containing the data to encode.

| Command | Barcode Type | Data Format |
|---------|--------------|-------------|
| `Ean13` | EAN-13 | 12-13 digits |
| `Ean8` | EAN-8 | 7-8 digits |
| `Upca` | UPC-A | 11-12 digits |
| `Upce` | UPC-E | 6-8 digits |
| `Code39` | Code 39 | Alphanumeric + symbols |
| `Codabar` | Codabar | Digits + A-D start/stop |
| `Itf` | ITF (Interleaved 2 of 5) | Even number of digits |

**Example:**
```json
{ "command": "Ean13", "parameters": "4006381333931" }
{ "command": "Code39", "parameters": "ABC123" }
```

---

### 2D Code Commands (String Parameter)

| Command | Code Type | Notes |
|---------|-----------|-------|
| `Qrcode` | QR Code | UTF-8 string |
| `GS1Databar2d` | GS1 DataBar | GS1 formatted string |
| `Pdf417` | PDF417 | Binary/text data |
| `Maxicode` | MaxiCode | Structured data |
| `DataMatrix` | Data Matrix | Binary/text data |
| `Aztec` | Aztec Code | Binary/text data |

**Example:**
```json
{ "command": "Qrcode", "parameters": "https://example.com/receipt/12345" }
{ "command": "Pdf417", "parameters": "Invoice data here" }
```

---

### Enum-Based Commands

#### PageCode

Set character code page for international character support.

```json
{ "command": "PageCode", "parameters": "PC437" }
```

**Valid Values:**
```
PC437, Katakana, PC850, PC860, PC863, PC865, Hiragana, PC851, PC853,
PC857, PC737, ISO8859_7, WPC1252, PC866, PC852, PC858, PC720, WPC775,
PC855, PC861, PC862, PC864, PC869, ISO8859_2, ISO8859_15, PC1098,
PC1118, PC1119, PC1125, WPC1250, WPC1251, WPC1253, WPC1254, WPC1255,
WPC1256, WPC1257, WPC1258, KZ1048
```

---

#### CharacterSet

Set character set for language-specific characters.

```json
{ "command": "CharacterSet", "parameters": "USA" }
```

**Valid Values:**
```
USA, France, Germany, UK, Denmark1, Sweden, Italy, Spain1, Japan,
Norway, Denmark2, Spain2, LatinAmerica, Korea, SloveniaCroatia, China,
Vietnam, Arabia, IndiaDevanagari, IndiaBengali, IndiaTamil, IndiaTelugu,
IndiaAssamese, IndiaOriya, IndiaKannada, IndiaMalayalam, IndiaGujarati,
IndiaPunjabi, IndiaMarathi
```

---

#### Underline

Set underline mode.

```json
{ "command": "Underline", "parameters": "Single" }
```

**Valid Values:**
```
None, Single, Double
```

---

#### Font

Set font face.

```json
{ "command": "Font", "parameters": "A" }
```

**Valid Values:**
```
A, B, C
```

---

#### Justify

Set text alignment.

```json
{ "command": "Justify", "parameters": "CENTER" }
```

**Valid Values:**
```
LEFT, CENTER, RIGHT
```

---

#### CashDrawer

Open cash drawer.

```json
{ "command": "CashDrawer", "parameters": "Pin2" }
```

**Valid Values:**
```
Pin2, Pin5
```

---

## Complete JSON Schema

For Rust `serde` deserialization, the complete schema:

```rust
// Enums
enum PageCode {
    PC437, Katakana, PC850, PC860, PC863, PC865, Hiragana, PC851, PC853,
    PC857, PC737, ISO8859_7, WPC1252, PC866, PC852, PC858, PC720, WPC775,
    PC855, PC861, PC862, PC864, PC869, ISO8859_2, ISO8859_15, PC1098,
    PC1118, PC1119, PC1125, WPC1250, WPC1251, WPC1253, WPC1254, WPC1255,
    WPC1256, WPC1257, WPC1258, KZ1048
}

enum CharacterSet {
    USA, France, Germany, UK, Denmark1, Sweden, Italy, Spain1, Japan,
    Norway, Denmark2, Spain2, LatinAmerica, Korea, SloveniaCroatia, China,
    Vietnam, Arabia, IndiaDevanagari, IndiaBengali, IndiaTamil, IndiaTelugu,
    IndiaAssamese, IndiaOriya, IndiaKannada, IndiaMalayalam, IndiaGujarati,
    IndiaPunjabi, IndiaMarathi
}

enum UnderlineMode { None, Single, Double }
enum Font { A, B, C }
enum Justify { LEFT, CENTER, RIGHT }
enum CashDrawer { Pin2, Pin5 }

// Command (tagged union on "command" field)
#[serde(tag = "command")]
enum Command {
    // No parameters
    Print,
    Init,
    Reset,
    Cut,
    PartialCut,
    PrintCut,
    ResetSize,
    ResetLineSpacing,

    // Boolean parameters
    Bold { parameters: bool },
    DoubleStrike { parameters: bool },
    Flip { parameters: bool },
    Reverse { parameters: bool },
    Smoothing { parameters: bool },
    Feed { parameters: bool },
    UpsideDown { parameters: bool },

    // Number parameters
    Feeds { parameters: u32 },
    LineSpacing { parameters: u32 },

    // String parameters
    Write { parameters: String },
    Writeln { parameters: String },

    // Tuple parameters
    Size { parameters: (u8, u8) },

    // Enum parameters
    PageCode { parameters: PageCode },
    CharacterSet { parameters: CharacterSet },
    Underline { parameters: UnderlineMode },
    Font { parameters: Font },
    Justify { parameters: Justify },
    CashDrawer { parameters: CashDrawer },

    // Barcodes (string parameter)
    Ean13 { parameters: String },
    Ean8 { parameters: String },
    Upca { parameters: String },
    Upce { parameters: String },
    Code39 { parameters: String },
    Codabar { parameters: String },
    Itf { parameters: String },

    // 2D codes (string parameter)
    Qrcode { parameters: String },
    GS1Databar2d { parameters: String },
    Pdf417 { parameters: String },
    Maxicode { parameters: String },
    DataMatrix { parameters: String },
    Aztec { parameters: String },
}

// Request body for POST /print
struct PrintRequest {
    commands: Vec<Command>,
}

// Request body for POST /print/test
struct TestPrintRequest {
    test_line: String,
    test_page: bool,
}

// Response for all endpoints
struct PrinterStatus {
    is_connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}
```

---

## Example Payloads

### Simple Receipt

```json
{
  "commands": [
    { "command": "Init" },
    { "command": "Justify", "parameters": "CENTER" },
    { "command": "Bold", "parameters": true },
    { "command": "Size", "parameters": [2, 2] },
    { "command": "Writeln", "parameters": "STORE NAME" },
    { "command": "ResetSize" },
    { "command": "Bold", "parameters": false },
    { "command": "Writeln", "parameters": "123 Main Street" },
    { "command": "Writeln", "parameters": "Tel: 555-1234" },
    { "command": "Feeds", "parameters": 1 },
    { "command": "Justify", "parameters": "LEFT" },
    { "command": "Writeln", "parameters": "--------------------------------" },
    { "command": "Writeln", "parameters": "Item 1              P100.00" },
    { "command": "Writeln", "parameters": "Item 2              P200.00" },
    { "command": "Writeln", "parameters": "--------------------------------" },
    { "command": "Bold", "parameters": true },
    { "command": "Writeln", "parameters": "TOTAL               P300.00" },
    { "command": "Bold", "parameters": false },
    { "command": "Feeds", "parameters": 2 },
    { "command": "Justify", "parameters": "CENTER" },
    { "command": "Qrcode", "parameters": "https://example.com/r/12345" },
    { "command": "Feeds", "parameters": 1 },
    { "command": "Writeln", "parameters": "Thank you!" },
    { "command": "Feeds", "parameters": 3 },
    { "command": "PrintCut" }
  ]
}
```

### Open Cash Drawer

```json
{
  "commands": [
    { "command": "CashDrawer", "parameters": "Pin2" }
  ]
}
```

### Print with QR Code Only

```json
{
  "commands": [
    { "command": "Init" },
    { "command": "Justify", "parameters": "CENTER" },
    { "command": "Qrcode", "parameters": "https://example.com/verify/abc123" },
    { "command": "Feeds", "parameters": 4 },
    { "command": "PrintCut" }
  ]
}
```

---

## Error Handling

### Expected Error Responses

When printer is disconnected or error occurs:

```json
{
  "is_connected": false,
  "error": "Printer not found on USB"
}
```

```json
{
  "is_connected": false,
  "error": "Paper out"
}
```

### HTTP Status Codes

| Status | Meaning |
|--------|---------|
| `200` | Success (check `is_connected` for printer status) |
| `400` | Invalid JSON or malformed command |
| `500` | Internal service error |

---

## Integration Notes

### Browser Calling Pattern

The REIKA web application calls the print service from browser JavaScript:

```typescript
// Check status
const status = await fetch('http://localhost:55000/print/test').then(r => r.json());

// Send commands
await fetch('http://localhost:55000/print', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ commands: [...] })
});
```

### CORS Requirements

Since the browser makes cross-origin requests from the REIKA domain to `localhost:55000`, the service must return appropriate CORS headers:

```
Access-Control-Allow-Origin: *
Access-Control-Allow-Methods: GET, POST, OPTIONS
Access-Control-Allow-Headers: Content-Type
```

### Mixed Content Consideration

If REIKA is served over HTTPS, browsers may block requests to `http://localhost:55000`. Options:

1. Browsers typically allow `localhost` as an exception
2. Optionally support HTTPS with self-signed certificate

---

## Source Files (REIKA Repository)

| File | Description |
|------|-------------|
| `src/lib/model/schemas/receiptSchema.ts` | Zod schema definitions for commands |
| `src/lib/model/print.ts` | Printer status type definitions |
| `src/routes/diagnose/PrinterTest.svelte` | Test UI component |
| `src/routes/diagnose/+page.svelte` | Diagnostics page |
