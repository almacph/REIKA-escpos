use base64::{engine::general_purpose::STANDARD, Engine as _};

use crate::models::{BitImage, CashDrawer, CashDrawerPulse};

const ESC: u8 = 0x1B;
const GS: u8 = 0x1D;
const RASTER_BAND_HEIGHT: u32 = 24;

pub fn encode_cash_drawer_pulse(pulse: &CashDrawerPulse) -> Vec<u8> {
    let pin = match pulse.pin {
        CashDrawer::Pin2 => 0,
        CashDrawer::Pin5 => 1,
    };
    vec![ESC, 0x70, pin, (pulse.pulse_ms / 2) as u8, 0xFF]
}

pub fn encode_bit_image(image: &BitImage) -> Result<Vec<u8>, String> {
    let raster = STANDARD
        .decode(&image.data)
        .map_err(|error| format!("invalid raster data: {error}"))?;
    let width_bytes = image.width.div_ceil(8) as usize;
    let mut output =
        Vec::with_capacity(raster.len() + 8 * image.height.div_ceil(RASTER_BAND_HEIGHT) as usize);

    for start_row in (0..image.height).step_by(RASTER_BAND_HEIGHT as usize) {
        let band_height = (image.height - start_row).min(RASTER_BAND_HEIGHT);
        let start = start_row as usize * width_bytes;
        let end = start + band_height as usize * width_bytes;
        let width_bytes_u16 = width_bytes as u16;
        let height_u16 = band_height as u16;
        output.extend_from_slice(&[
            GS,
            0x76,
            0x30,
            0x00,
            (width_bytes_u16 & 0xFF) as u8,
            (width_bytes_u16 >> 8) as u8,
            (height_u16 & 0xFF) as u8,
            (height_u16 >> 8) as u8,
        ]);
        output.extend_from_slice(&raster[start..end]);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_pulse_pin_and_duration() {
        for (pin, pin_byte) in [(CashDrawer::Pin2, 0), (CashDrawer::Pin5, 1)] {
            for (pulse_ms, units) in [(100, 50), (150, 75), (200, 100)] {
                let pulse = CashDrawerPulse {
                    pin: pin.clone(),
                    pulse_ms,
                };
                assert_eq!(
                    encode_cash_drawer_pulse(&pulse),
                    [0x1B, 0x70, pin_byte, units, 0xFF]
                );
            }
        }
    }

    #[test]
    fn splits_raster_into_24_row_bands() {
        let image = BitImage {
            width: 9,
            height: 25,
            data: STANDARD.encode(vec![0xAA; 50]),
        };
        let bytes = encode_bit_image(&image).unwrap();
        assert_eq!(&bytes[..8], &[0x1D, 0x76, 0x30, 0, 2, 0, 24, 0]);
        assert_eq!(&bytes[8 + 48..16 + 48], &[0x1D, 0x76, 0x30, 0, 2, 0, 1, 0]);
        assert_eq!(bytes.len(), 8 + 48 + 8 + 2);
    }
}
