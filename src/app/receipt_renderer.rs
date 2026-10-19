use crate::models::{Command, JustifyMode, UnderlineMode};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use egui::{Color32, FontId, Pos2, Rect, RichText, Stroke, Ui, Vec2};

/// Thermal receipt paper width in characters (typical 58mm paper)
const RECEIPT_WIDTH_CHARS: usize = 32;
/// Character width in pixels for rendering
const CHAR_WIDTH: f32 = 8.0;
/// Line height in pixels
const LINE_HEIGHT: f32 = 16.0;
/// Base font size
const BASE_FONT_SIZE: f32 = 14.0;

/// Represents the current text formatting state
#[derive(Clone)]
struct TextState {
    bold: bool,
    underline: UnderlineMode,
    reverse: bool,
    justify: JustifyMode,
    size_width: u8,
    size_height: u8,
}

impl Default for TextState {
    fn default() -> Self {
        Self {
            bold: false,
            underline: UnderlineMode::None,
            reverse: false,
            justify: JustifyMode::LEFT,
            size_width: 1,
            size_height: 1,
        }
    }
}

/// A rendered line of text with its formatting
struct RenderedLine {
    text: String,
    state: TextState,
    image: Option<RenderedImage>,
}

struct RenderedImage {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

/// Renders print commands as a receipt mockup
pub struct ReceiptRenderer {
    lines: Vec<RenderedLine>,
    current_line: String,
    state: TextState,
}

impl ReceiptRenderer {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            current_line: String::new(),
            state: TextState::default(),
        }
    }

    /// Process commands and build the receipt mockup
    pub fn process_commands(&mut self, commands: &[Command]) {
        for cmd in commands {
            self.process_command(cmd);
        }
        // Flush any remaining text
        self.flush_line();
    }

    fn process_command(&mut self, cmd: &Command) {
        match cmd {
            Command::Write(text) => {
                self.current_line.push_str(text);
            }
            Command::Writeln(text) => {
                self.current_line.push_str(text);
                self.flush_line();
            }
            Command::Feed(_) | Command::Feeds(_) => {
                self.flush_line();
                // Add empty line for feed
                self.lines.push(RenderedLine {
                    text: String::new(),
                    state: self.state.clone(),
                    image: None,
                });
            }
            Command::Bold(enabled) => {
                self.state.bold = *enabled;
            }
            Command::Italic(enabled) => {
                let _ = enabled;
            }
            Command::Underline(mode) => {
                self.state.underline = mode.clone();
            }
            Command::Reverse(enabled) => {
                self.state.reverse = *enabled;
            }
            Command::Justify(mode) => {
                self.state.justify = mode.clone();
            }
            Command::Size((w, h)) => {
                self.state.size_width = *w;
                self.state.size_height = *h;
            }
            Command::ResetSize(_) => {
                self.state.size_width = 1;
                self.state.size_height = 1;
            }
            Command::Init(_) | Command::Reset(_) => {
                self.state = TextState::default();
            }
            Command::Cut(_) | Command::PartialCut(_) | Command::PrintCut(_) => {
                self.flush_line();
                // Add a visual cut indicator
                self.lines.push(RenderedLine {
                    text: "- - - - - - - - - - - - - - - -".to_string(),
                    state: TextState::default(),
                    image: None,
                });
            }
            Command::Qrcode(data) => {
                self.flush_line();
                self.lines.push(RenderedLine {
                    text: format!("[QR: {}]", Self::truncate(data, 20)),
                    state: TextState {
                        justify: JustifyMode::CENTER,
                        ..self.state.clone()
                    },
                    image: None,
                });
            }
            Command::Ean13(data)
            | Command::Ean8(data)
            | Command::Upca(data)
            | Command::Upce(data)
            | Command::Code39(data)
            | Command::Codabar(data)
            | Command::Itf(data) => {
                self.flush_line();
                self.lines.push(RenderedLine {
                    text: format!("[BARCODE: {}]", data),
                    state: TextState {
                        justify: JustifyMode::CENTER,
                        ..self.state.clone()
                    },
                    image: None,
                });
            }
            Command::BitImage(image) => {
                self.flush_line();
                if let Ok(data) = STANDARD.decode(&image.data) {
                    self.lines.push(RenderedLine {
                        text: String::new(),
                        state: self.state.clone(),
                        image: Some(RenderedImage {
                            width: image.width,
                            height: image.height,
                            data,
                        }),
                    });
                }
            }
            // Ignore other commands that don't affect visual output
            _ => {}
        }
    }

    fn flush_line(&mut self) {
        if !self.current_line.is_empty() {
            self.lines.push(RenderedLine {
                text: std::mem::take(&mut self.current_line),
                state: self.state.clone(),
                image: None,
            });
        }
    }

    fn truncate(s: &str, max: usize) -> &str {
        if s.len() > max {
            &s[..max]
        } else {
            s
        }
    }

    /// Render the receipt mockup to the UI
    pub fn render(&self, ui: &mut Ui) {
        let receipt_width = RECEIPT_WIDTH_CHARS as f32 * CHAR_WIDTH + 32.0;

        let content_width = receipt_width - 32.0;
        // Calculate total height accounting for variable line sizes and raster images.
        let total_content_height: f32 = self
            .lines
            .iter()
            .map(|line| {
                line.image.as_ref().map_or(
                    LINE_HEIGHT * line.state.size_height.max(1) as f32,
                    |image| content_width * image.height as f32 / image.width.max(1) as f32 + 4.0,
                )
            })
            .sum();

        // Draw receipt background
        let available_rect = ui.available_rect_before_wrap();
        let receipt_rect = Rect::from_min_size(
            Pos2::new(
                available_rect.center().x - receipt_width / 2.0,
                available_rect.min.y,
            ),
            Vec2::new(receipt_width, total_content_height + 40.0),
        );

        ui.painter().rect_filled(
            receipt_rect,
            4.0,
            Color32::from_rgb(255, 252, 240), // Slightly off-white paper color
        );

        // Draw receipt border
        ui.painter().rect_stroke(
            receipt_rect,
            4.0,
            Stroke::new(1.0, Color32::from_rgb(200, 200, 200)),
            egui::StrokeKind::Inside,
        );

        // Render each line
        let content_start = receipt_rect.min + Vec2::new(16.0, 20.0);
        let mut current_y = content_start.y;
        for line in self.lines.iter() {
            let y = current_y;

            if let Some(image) = &line.image {
                let image_width = content_width.min(image.width as f32 * 2.0);
                let image_height = image_width * image.height as f32 / image.width.max(1) as f32;
                let x = content_start.x + (content_width - image_width) / 2.0;
                let pixel_width = image_width / image.width.max(1) as f32;
                let pixel_height = image_height / image.height.max(1) as f32;
                for pixel_y in 0..image.height {
                    for pixel_x in 0..image.width {
                        let byte_index = pixel_y as usize * image.width.div_ceil(8) as usize
                            + pixel_x as usize / 8;
                        if image
                            .data
                            .get(byte_index)
                            .is_some_and(|byte| byte & (0x80 >> (pixel_x % 8)) != 0)
                        {
                            ui.painter().rect_filled(
                                Rect::from_min_size(
                                    Pos2::new(
                                        x + pixel_x as f32 * pixel_width,
                                        y + pixel_y as f32 * pixel_height,
                                    ),
                                    Vec2::new(pixel_width.ceil(), pixel_height.ceil()),
                                ),
                                0.0,
                                Color32::BLACK,
                            );
                        }
                    }
                }
                current_y += image_height + 4.0;
                continue;
            }

            // Calculate font size based on size multiplier
            let font_size = BASE_FONT_SIZE * line.state.size_height.max(1) as f32;

            // Determine text color based on reverse mode
            let (text_color, bg_color) = if line.state.reverse {
                (Color32::WHITE, Color32::BLACK)
            } else {
                (Color32::BLACK, Color32::TRANSPARENT)
            };

            // Build rich text
            let mut rich_text = RichText::new(&line.text)
                .size(font_size)
                .color(text_color)
                .monospace();

            if line.state.bold {
                rich_text = rich_text.strong();
            }

            match line.state.underline {
                UnderlineMode::Single | UnderlineMode::Double => {
                    rich_text = rich_text.underline();
                }
                _ => {}
            }

            // Calculate x position based on justification
            let text_width =
                line.text.len() as f32 * CHAR_WIDTH * line.state.size_width.max(1) as f32;
            let x = match line.state.justify {
                JustifyMode::LEFT => content_start.x,
                JustifyMode::CENTER => content_start.x + (content_width - text_width) / 2.0,
                JustifyMode::RIGHT => content_start.x + content_width - text_width,
            };

            // Draw background if reverse
            if line.state.reverse {
                let bg_rect = Rect::from_min_size(
                    Pos2::new(x - 2.0, y - 2.0),
                    Vec2::new(text_width + 4.0, font_size + 4.0),
                );
                ui.painter().rect_filled(bg_rect, 0.0, bg_color);
            }

            // Draw the text
            ui.painter().text(
                Pos2::new(x, y),
                egui::Align2::LEFT_TOP,
                &line.text,
                FontId::monospace(font_size),
                text_color,
            );

            // Advance y position by this line's height
            current_y += LINE_HEIGHT * line.state.size_height.max(1) as f32;
        }

        // Reserve the space
        ui.allocate_rect(receipt_rect, egui::Sense::hover());
    }
}

/// Render a preview of print commands in a scrollable area
pub fn render_receipt_preview(ui: &mut Ui, commands: &[Command]) {
    let mut renderer = ReceiptRenderer::new();
    renderer.process_commands(commands);

    egui::ScrollArea::vertical()
        .max_height(400.0)
        .show(ui, |ui| {
            renderer.render(ui);
        });
}
