use chrono::Local;
use crate::models::{Command, Font, JustifyMode, UnderlineMode};

/// Tracks ESC/POS formatting state to safely inject markers mid-stream
#[derive(Debug, Clone)]
pub struct FormattingState {
    pub bold: bool,
    pub underline: UnderlineMode,
    pub double_strike: bool,
    pub reverse: bool,
    pub justify: JustifyMode,
    pub size: (u8, u8),
    pub smoothing: bool,
    pub flip: bool,
    pub upside_down: bool,
    pub font: Font,
}

impl Default for FormattingState {
    fn default() -> Self {
        Self {
            bold: false,
            underline: UnderlineMode::None,
            double_strike: false,
            reverse: false,
            justify: JustifyMode::LEFT,
            size: (1, 1),
            smoothing: false,
            flip: false,
            upside_down: false,
            font: Font::A,
        }
    }
}

impl FormattingState {
    /// Update state from a command. Returns true if the command was a formatting command.
    pub fn apply(&mut self, command: &Command) {
        match command {
            Command::Bold(v) => self.bold = *v,
            Command::Underline(v) => self.underline = v.clone(),
            Command::DoubleStrike(v) => self.double_strike = *v,
            Command::Reverse(v) => self.reverse = *v,
            Command::Justify(v) => self.justify = v.clone(),
            Command::Size(v) => self.size = *v,
            Command::ResetSize(_) => self.size = (1, 1),
            Command::Smoothing(v) => self.smoothing = *v,
            Command::Flip(v) => self.flip = *v,
            Command::UpsideDown(v) => self.upside_down = *v,
            Command::Font(v) => self.font = v.clone(),
            Command::Init(_) | Command::Reset(_) => *self = Self::default(),
            _ => {}
        }
    }

    /// Generate commands to reset formatting to defaults
    pub fn reset_commands() -> Vec<Command> {
        vec![
            Command::Bold(false),
            Command::Underline(UnderlineMode::None),
            Command::DoubleStrike(false),
            Command::Reverse(false),
            Command::Justify(JustifyMode::LEFT),
            Command::ResetSize(None),
            Command::Smoothing(false),
            Command::Flip(false),
            Command::UpsideDown(false),
            Command::Font(Font::A),
        ]
    }

    /// Generate commands to restore the current state (only non-default values)
    pub fn restore_commands(&self) -> Vec<Command> {
        let mut cmds = Vec::new();
        if self.bold {
            cmds.push(Command::Bold(true));
        }
        if !matches!(self.underline, UnderlineMode::None) {
            cmds.push(Command::Underline(self.underline.clone()));
        }
        if self.double_strike {
            cmds.push(Command::DoubleStrike(true));
        }
        if self.reverse {
            cmds.push(Command::Reverse(true));
        }
        if !matches!(self.justify, JustifyMode::LEFT) {
            cmds.push(Command::Justify(self.justify.clone()));
        }
        if self.size != (1, 1) {
            cmds.push(Command::Size(self.size));
        }
        if self.smoothing {
            cmds.push(Command::Smoothing(true));
        }
        if self.flip {
            cmds.push(Command::Flip(true));
        }
        if self.upside_down {
            cmds.push(Command::UpsideDown(true));
        }
        if !matches!(self.font, Font::A) {
            cmds.push(Command::Font(self.font.clone()));
        }
        cmds
    }
}

/// Check if a command produces visible content on the receipt
fn is_content_command(command: &Command) -> bool {
    matches!(
        command,
        Command::Write(_)
            | Command::Writeln(_)
            | Command::Ean13(_)
            | Command::Ean8(_)
            | Command::Upca(_)
            | Command::Upce(_)
            | Command::Code39(_)
            | Command::Codabar(_)
            | Command::Itf(_)
            | Command::Qrcode(_)
            | Command::GS1Databar2d(_)
            | Command::Pdf417(_)
            | Command::MaxiCode(_)
            | Command::DataMatrix(_)
            | Command::Aztec(_)
    )
}

/// Build the reprint marker commands block (reversed white-on-black text)
fn build_reprint_marker_commands(timestamp: &str) -> Vec<Command> {
    vec![
        Command::Justify(JustifyMode::CENTER),
        Command::Reverse(true),
        Command::Writeln("================================".to_string()),
        Command::Writeln("     ** REPRINT COPY **".to_string()),
        Command::Writeln(format!("  {}", timestamp)),
        Command::Writeln("  REIKA-escpos".to_string()),
        Command::Writeln("================================".to_string()),
        Command::Reverse(false),
        Command::Justify(JustifyMode::LEFT),
    ]
}

/// Find the index at which to split commands for the midpoint marker.
/// Counts content-producing commands and returns the command index at floor(count/2).
fn find_content_midpoint(commands: &[Command]) -> usize {
    let content_count = commands.iter().filter(|c| is_content_command(c)).count();
    if content_count == 0 {
        return commands.len();
    }

    let target = content_count / 2;
    let mut seen = 0;
    for (i, cmd) in commands.iter().enumerate() {
        if is_content_command(cmd) {
            seen += 1;
            if seen > target {
                return i;
            }
        }
    }
    commands.len()
}

/// Inject reprint markers at top, middle, and bottom of a command stream.
///
/// Algorithm:
/// 1. [Init] + [marker_top] + [reset_to_default]
/// 2. [first_half_of_original_commands]
/// 3. [save_state] + [reset_to_default] + [marker_mid] + [restore_saved_state]
/// 4. [second_half_of_original_commands]
/// 5. [reset_to_default] + [marker_bottom] + [PrintCut]
pub fn inject_reprint_markers(commands: Vec<Command>) -> Vec<Command> {
    let timestamp = Local::now().format("%Y-%m-%d  %H:%M:%S").to_string();
    let marker = build_reprint_marker_commands(&timestamp);

    // Strip any trailing PrintCut/Cut from original commands — we add our own at the end
    let mut original: Vec<Command> = commands
        .into_iter()
        .filter(|c| !matches!(c, Command::PrintCut(_) | Command::Cut(_) | Command::PartialCut(_)))
        .collect();

    // Also strip leading Init — we add our own
    if matches!(original.first(), Some(Command::Init(_))) {
        original.remove(0);
    }

    let midpoint = find_content_midpoint(&original);
    let (first_half, second_half) = original.split_at(midpoint);

    // Scan formatting state up to midpoint
    let mut state = FormattingState::default();
    for cmd in first_half {
        state.apply(cmd);
    }

    let mut result = Vec::new();

    // 1. Init + top marker + reset
    result.push(Command::Init(None));
    result.extend(marker.clone());
    result.extend(FormattingState::reset_commands());

    // 2. First half of original
    result.extend(first_half.iter().cloned());

    // 3. State save → reset → mid marker → state restore
    result.extend(FormattingState::reset_commands());
    result.push(Command::Feed(true));
    result.extend(marker.clone());
    result.push(Command::Feed(true));
    result.extend(state.restore_commands());

    // 4. Second half of original
    result.extend(second_half.iter().cloned());

    // 5. Reset → bottom marker → PrintCut
    result.extend(FormattingState::reset_commands());
    result.push(Command::Feed(true));
    result.extend(marker);
    result.push(Command::PrintCut(None));

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_formatting_state_default() {
        let state = FormattingState::default();
        assert!(!state.bold);
        assert!(matches!(state.underline, UnderlineMode::None));
        assert!(matches!(state.justify, JustifyMode::LEFT));
        assert_eq!(state.size, (1, 1));
    }

    #[test]
    fn test_formatting_state_apply() {
        let mut state = FormattingState::default();
        state.apply(&Command::Bold(true));
        assert!(state.bold);
        state.apply(&Command::Size((2, 3)));
        assert_eq!(state.size, (2, 3));
        state.apply(&Command::Init(None));
        assert!(!state.bold);
        assert_eq!(state.size, (1, 1));
    }

    #[test]
    fn test_restore_only_emits_non_defaults() {
        let state = FormattingState::default();
        assert!(state.restore_commands().is_empty());

        let mut state2 = FormattingState::default();
        state2.apply(&Command::Bold(true));
        state2.apply(&Command::Justify(JustifyMode::CENTER));
        let cmds = state2.restore_commands();
        assert_eq!(cmds.len(), 2);
    }

    #[test]
    fn test_find_content_midpoint() {
        let commands = vec![
            Command::Bold(true),
            Command::Writeln("line 1".to_string()),
            Command::Writeln("line 2".to_string()),
            Command::Bold(false),
            Command::Writeln("line 3".to_string()),
            Command::Writeln("line 4".to_string()),
        ];
        // 4 content commands, target = 2, so midpoint at 3rd content command (index 4)
        let mid = find_content_midpoint(&commands);
        assert_eq!(mid, 4);
    }

    #[test]
    fn test_inject_reprint_markers_structure() {
        let commands = vec![
            Command::Init(None),
            Command::Writeln("Hello".to_string()),
            Command::Writeln("World".to_string()),
            Command::PrintCut(None),
        ];
        let result = inject_reprint_markers(commands);
        // Should start with Init
        assert!(matches!(result.first(), Some(Command::Init(None))));
        // Should end with PrintCut
        assert!(matches!(result.last(), Some(Command::PrintCut(None))));
        // Should contain "REPRINT COPY" markers (3 of them: top, mid, bottom)
        let reprint_count = result.iter().filter(|c| {
            matches!(c, Command::Writeln(text) if text.contains("REPRINT COPY"))
        }).count();
        assert_eq!(reprint_count, 3);
    }
}
