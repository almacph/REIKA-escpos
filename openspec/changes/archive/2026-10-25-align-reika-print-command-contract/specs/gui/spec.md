## ADDED Requirements

### Requirement: Consumer-Compatible Receipt Preview

The desktop receipt preview SHALL render legacy Italic commands as no-ops and display valid BitImage raster content in command order. Image display SHALL preserve aspect ratio and fit the available receipt width. Other supported text formatting and the existing receipt preview/reprint interaction SHALL remain unchanged.

#### Scenario: Legacy saved receipt
- **WHEN** a stored receipt contains Italic true and false around text
- **THEN** the preview opens successfully and shows regular text while preserving other supported styles

#### Scenario: Raster receipt
- **WHEN** a receipt contains a valid raster image between two text lines
- **THEN** the preview shows the image in that position without distortion
