## MODIFIED Requirements

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

## ADDED Requirements

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
