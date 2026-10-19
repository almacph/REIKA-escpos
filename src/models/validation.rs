use base64::{engine::general_purpose::STANDARD, Engine as _};

use super::{Command, Commands};

pub const MAX_JOB_BODY_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_IMAGE_WIDTH: u32 = 576;
pub const MAX_IMAGE_HEIGHT: u32 = 4096;
pub const MAX_IMAGE_BYTES: usize = 294_912;
/// escpos encodes 2D payload lengths in a two-byte pL/pH pair. Keep the
/// payload below the largest value accepted by its shared encoder, including
/// the command-specific four-byte padding.
pub const MAX_2D_DATA_BYTES: usize = 65_531;

impl Commands {
    /// Validate all deterministic command constraints before a printer is opened or retried.
    pub fn validate(&self) -> Result<(), String> {
        for (index, command) in self.commands.iter().enumerate() {
            match command {
                Command::Size((width, height))
                    if *width == 0 || *width > 8 || *height == 0 || *height > 8 =>
                {
                    return Err(format!(
                        "commands[{index}].parameters size width and height must be between 1 and 8"
                    ));
                }
                Command::Ean13(data)
                | Command::Ean8(data)
                | Command::Upca(data)
                | Command::Upce(data)
                | Command::Code39(data)
                | Command::Codabar(data)
                | Command::Itf(data)
                    if data.is_empty() =>
                {
                    return Err(format!(
                        "commands[{index}].parameters barcode data must not be empty"
                    ));
                }
                Command::Qrcode(data) if data.len() > 7090 => {
                    return Err(format!(
                        "commands[{index}].parameters QR code data is too long"
                    ));
                }
                Command::GS1Databar2d(data)
                | Command::Pdf417(data)
                | Command::MaxiCode(data)
                | Command::DataMatrix(data)
                | Command::Aztec(data)
                    if data.len() > MAX_2D_DATA_BYTES =>
                {
                    return Err(format!(
                        "commands[{index}].parameters 2D data exceeds {MAX_2D_DATA_BYTES} bytes"
                    ));
                }
                Command::CashDrawerPulse(pulse) if !matches!(pulse.pulse_ms, 100 | 150 | 200) => {
                    return Err(format!(
                        "commands[{index}].parameters.pulseMs must be 100, 150, or 200"
                    ));
                }
                Command::BitImage(image) => {
                    validate_image(index, image.width, image.height, &image.data)?
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn validate_image(index: usize, width: u32, height: u32, data: &str) -> Result<(), String> {
    if !(1..=MAX_IMAGE_WIDTH).contains(&width) {
        return Err(format!(
            "commands[{index}].parameters.width must be between 1 and {MAX_IMAGE_WIDTH}"
        ));
    }
    if !(1..=MAX_IMAGE_HEIGHT).contains(&height) {
        return Err(format!(
            "commands[{index}].parameters.height must be between 1 and {MAX_IMAGE_HEIGHT}"
        ));
    }

    let bytes_per_row = width.div_ceil(8) as usize;
    let expected_bytes = bytes_per_row
        .checked_mul(height as usize)
        .ok_or_else(|| format!("commands[{index}].parameters image dimensions overflow"))?;
    if expected_bytes > MAX_IMAGE_BYTES {
        return Err(format!(
            "commands[{index}].parameters image exceeds {MAX_IMAGE_BYTES} decoded bytes"
        ));
    }

    let decoded = STANDARD
        .decode(data)
        .map_err(|error| format!("commands[{index}].parameters.data is invalid base64: {error}"))?;
    if decoded.len() != expected_bytes {
        return Err(format!(
            "commands[{index}].parameters.data must decode to {expected_bytes} bytes, got {}",
            decoded.len()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{BitImage, CashDrawer, CashDrawerPulse};

    #[test]
    fn accepts_reika_pulse_values_and_exact_raster_length() {
        let commands = Commands {
            commands: vec![
                Command::CashDrawerPulse(CashDrawerPulse {
                    pin: CashDrawer::Pin2,
                    pulse_ms: 150,
                }),
                Command::BitImage(BitImage {
                    width: 9,
                    height: 2,
                    data: "AAAAAA==".to_string(),
                }),
            ],
        };
        assert!(commands.validate().is_ok());
    }

    #[test]
    fn rejects_invalid_pulse_and_raster_dimensions() {
        let pulse = Commands {
            commands: vec![Command::CashDrawerPulse(CashDrawerPulse {
                pin: CashDrawer::Pin5,
                pulse_ms: 125,
            })],
        };
        assert!(pulse.validate().is_err());

        let image = Commands {
            commands: vec![Command::BitImage(BitImage {
                width: 577,
                height: 1,
                data: "AA==".to_string(),
            })],
        };
        assert!(image.validate().is_err());

        let size = Commands {
            commands: vec![Command::Size((0, 9))],
        };
        assert!(size.validate().is_err());

        let barcode = Commands {
            commands: vec![Command::Ean13(String::new())],
        };
        assert!(barcode.validate().is_err());
    }

    #[test]
    fn rejects_oversized_late_2d_commands_before_transport() {
        let data = "x".repeat(MAX_2D_DATA_BYTES + 1);
        for command in [
            Command::GS1Databar2d(data.clone()),
            Command::Pdf417(data.clone()),
            Command::MaxiCode(data.clone()),
            Command::DataMatrix(data.clone()),
            Command::Aztec(data.clone()),
        ] {
            let commands = Commands {
                commands: vec![Command::Writeln("prefix".into()), command],
            };
            assert!(commands.validate().is_err());
        }
    }
}
