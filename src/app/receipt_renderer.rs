use crate::models::{Command, JustifyMode, UnderlineMode};
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
                });
            }
            Command::Bold(enabled) => {
                self.state.bold = *enabled;
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
                });
            }
            Command::Qrcode(data) => {
                self.flush_line();
                self.lines.push(RenderedLine {
                    text: format!("[QR: {}]", Self::truncate(data, 20)),
                    state: TextState { justify: JustifyMode::CENTER, ..self.state.clone() },
                });
            }
            Command::Ean13(data) | Command::Ean8(data) | Command::Upca(data)
            | Command::Upce(data) | Command::Code39(data) | Command::Codabar(data)
            | Command::Itf(data) => {
                self.flush_line();
                self.lines.push(RenderedLine {
                    text: format!("[BARCODE: {}]", data),
                    state: TextState { justify: JustifyMode::CENTER, ..self.state.clone() },
                });
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

        // Draw receipt background
        let available_rect = ui.available_rect_before_wrap();
        let receipt_rect = Rect::from_min_size(
            Pos2::new(
                available_rect.center().x - receipt_width / 2.0,
                available_rect.min.y,
            ),
            Vec2::new(receipt_width, self.lines.len() as f32 * LINE_HEIGHT + 40.0),
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
        );

        // Render each line
        let content_start = receipt_rect.min + Vec2::new(16.0, 20.0);
        let content_width = receipt_width - 32.0;

        for (i, line) in self.lines.iter().enumerate() {
            let y = content_start.y + (i as f32 * LINE_HEIGHT * line.state.size_height as f32);

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
            let text_width = line.text.len() as f32 * CHAR_WIDTH * line.state.size_width.max(1) as f32;
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
