## ADDED Requirements

### Requirement: Consumer Command Parity in Reprints

Normal printing, API reprints, and GUI reprints SHALL use the same command validation and encoding semantics. Reprint midpoint calculation SHALL count each BitImage as one content-producing command in addition to the existing text and barcode commands. Legacy Italic commands SHALL NOT contribute visible content, emit bytes, or be restored around markers. Existing top/middle/bottom markers, other formatting restoration, recovery for later requests after transport errors, and omission of reprints from print history SHALL remain in effect. A failed print request SHALL NOT be replayed after an ambiguous printer I/O error.

#### Scenario: Image participates in midpoint
- **WHEN** the source receipt contains one BitImage followed by one Writeln
- **THEN** the middle marker is inserted between those two content commands
- **AND** markers also appear at the top and bottom

#### Scenario: Legacy italic reprint
- **WHEN** either reprint entry point processes a saved receipt with Italic commands
- **THEN** the receipt and markers contain no italic command bytes
- **AND** non-italic formatting is preserved

#### Scenario: Invalid GUI reprint
- **WHEN** a saved receipt fails preflight validation
- **THEN** the GUI receives an error without USB output or an infinite input-error retry
- **AND** no reprint history entry is added
