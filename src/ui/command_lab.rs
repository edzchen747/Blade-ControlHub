use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::ipc::protocol::CommandLabStatus;
use crate::win::system::usbpcap::capture::CapturedCommand;

pub const COMMAND_LAB_CODE_PREVIEW_COMMANDS: usize = 2;
pub const COMMAND_LAB_COMMAND_ARGS_PREVIEW: usize = 3;
pub const COMMAND_LAB_TOO_MANY_NOTICE_DURATION: Duration = Duration::from_secs(3);
pub const NEW_ROW_ERROR_MESSAGE: &str = "Complete the last row to add more";

#[derive(Default, Clone)]
pub struct CommandLabRow {
    pub command: String,
    pub captured_commands: Vec<CapturedCommand>,
    pub too_many_commands: bool,
    restore_captured_commands: Vec<CapturedCommand>,
    too_many_shown_at: Option<Instant>,
}

impl CommandLabRow {
    fn show_too_many_notice(&mut self, captured: Vec<CapturedCommand>) {
        self.restore_captured_commands = std::mem::take(&mut self.captured_commands);
        self.captured_commands = captured;
        self.too_many_commands = true;
        self.too_many_shown_at = Some(Instant::now());
    }

    pub fn expire_too_many_notice(&mut self, now: Instant) -> bool {
        let Some(shown_at) = self.too_many_shown_at else {
            return false;
        };
        if now.duration_since(shown_at) < COMMAND_LAB_TOO_MANY_NOTICE_DURATION {
            return false;
        }
        self.too_many_commands = false;
        self.too_many_shown_at = None;
        self.captured_commands = std::mem::take(&mut self.restore_captured_commands);
        true
    }
}

#[derive(Default)]
pub struct CommandLab {
    pub rows: Vec<CommandLabRow>,
    pub recording_row_idx: Option<usize>,
    pub show_help: bool,
}

impl CommandLab {
    pub fn new() -> Self {
        Self {
            rows: vec![CommandLabRow::default()],
            recording_row_idx: None,
            show_help: false,
        }
    }

    pub fn recording_row_idx(&self) -> Option<usize> {
        self.recording_row_idx
    }

    pub fn set_recording_row_idx(&mut self, idx: Option<usize>) {
        self.recording_row_idx = idx;
    }

    pub fn begin_capture(&mut self, idx: usize) {
        self.recording_row_idx = Some(idx);
        if let Some(row) = self.rows.get_mut(idx) {
            row.too_many_commands = false;
            row.too_many_shown_at = None;
            row.restore_captured_commands.clear();
        }
    }

    pub fn set_captured_command_list(&mut self, commands: Vec<CapturedCommand>) {
        if let Some(idx) = self.recording_row_idx
            && let Some(row) = self.rows.get_mut(idx)
        {
            row.captured_commands = commands;
        }
    }

    pub fn apply_capture_result(&mut self, status: CommandLabStatus, commands: Vec<CapturedCommand>) {
        match status {
            CommandLabStatus::TooManyCommands => {
                if let Some(idx) = self.recording_row_idx
                    && let Some(row) = self.rows.get_mut(idx)
                {
                    row.show_too_many_notice(commands);
                }
            }
            _ => {
                if !commands.is_empty() {
                    self.set_captured_command_list(commands);
                }
            }
        }
    }

    /// Reconciles the rows with the persisted command library: fills existing
    /// rows that match a saved name and have no capture yet, and appends new
    /// rows for saved names that are not present. Fresh captures are never
    /// overwritten, and repeated refreshes do not duplicate rows. When
    /// entries are loaded, the pristine default row is dropped so the list
    /// starts at the saved commands instead of an empty row.
    pub fn populate_from_config(&mut self, saved: HashMap<String, Vec<CapturedCommand>>) {
        if !saved.is_empty()
            && self.rows.len() == 1
            && self.rows[0].command.is_empty()
            && self.rows[0].captured_commands.is_empty()
        {
            self.rows.clear();
        }
        for (name, commands) in saved {
            let mut found = false;
            for row in self.rows.iter_mut() {
                if row.command == name {
                    found = true;
                    if row.captured_commands.is_empty() {
                        row.captured_commands = commands.clone();
                    }
                    break;
                }
            }
            if !found {
                self.rows.push(CommandLabRow { command: name, captured_commands: commands, too_many_commands: false, ..CommandLabRow::default() });
            }
        }
    }

    pub fn is_recording(&self) -> bool {
        self.recording_row_idx.is_some()
    }

    pub fn row_name_is_duplicate(&self, idx: usize) -> bool {
        let Some(name) = self.rows.get(idx).map(|row| row.command.trim()) else {
            return false;
        };
        !name.is_empty() && self.rows.iter().filter(|row| row.command.trim() == name).count() > 1
    }

    pub fn row_ready_to_save(&self, idx: usize) -> bool {
        let Some(row) = self.rows.get(idx) else {
            return false;
        };
        !row.captured_commands.is_empty()
            && !row.too_many_commands
            && !row.command.trim().is_empty()
            && !self.row_name_is_duplicate(idx)
    }

    pub fn can_add_row(&self) -> bool {
        self.recording_row_idx.is_none()
            && self.rows.iter().all(|row| !row.command.is_empty())
            && self
                .rows
                .last()
                .is_some_and(|row| !row.captured_commands.is_empty())
    }

    pub fn add_row(&mut self) {
        self.rows.push(CommandLabRow::default());
    }

    pub fn remove_row(&mut self, idx: usize) {
        if self.recording_row_idx == Some(idx) {
            self.recording_row_idx = None;
        }
        if let Some(recording_row_idx) = self.recording_row_idx.as_mut()
            && *recording_row_idx > idx
        {
            *recording_row_idx -= 1;
        }
        self.rows.remove(idx);
        if self.rows.is_empty() {
            self.rows.push(CommandLabRow::default());
        }
    }
}

fn format_code_block_command(command: &CapturedCommand) -> String {
    format!("{:04X}", command.command)
}

pub fn format_command_full(command: &CapturedCommand) -> String {
    let mut text = format!("0x{:04X}", command.command);
    for arg in command.args.iter().take(COMMAND_LAB_COMMAND_ARGS_PREVIEW) {
        text.push_str(&format!(" {:02X}", arg));
    }
    if command.args.len() > COMMAND_LAB_COMMAND_ARGS_PREVIEW {
        text.push_str(" …");
    }
    text
}

pub fn command_lab_code_preview(commands: &[CapturedCommand]) -> String {
    let mut text = commands
        .iter()
        .take(COMMAND_LAB_CODE_PREVIEW_COMMANDS)
        .map(format_code_block_command)
        .collect::<Vec<_>>()
        .join(", ");
    let remaining = commands.len().saturating_sub(COMMAND_LAB_CODE_PREVIEW_COMMANDS);
    if remaining > 0 {
        text.push_str(&format!(" … {remaining} more"));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_lab_starts_with_one_empty_row() {
        let command_lab = CommandLab::new();

        assert_eq!(command_lab.rows.len(), 1);
        assert!(!command_lab.is_recording());
    }

    #[test]
    fn can_add_row_requires_no_active_recording() {
        let mut command_lab = CommandLab::new();
        command_lab.rows[0].command = "sleep 1".to_string();
        command_lab.rows[0].captured_commands = vec![command(0x0303, &[])];

        assert!(command_lab.can_add_row());

        command_lab.recording_row_idx = Some(0);
        assert!(!command_lab.can_add_row());
    }

    #[test]
    fn can_add_row_requires_complete_previous_rows() {
        let mut command_lab = CommandLab::new();
        command_lab.rows.push(CommandLabRow::default());

        assert!(!command_lab.can_add_row());

        command_lab.rows[0].command = "echo hi".to_string();
        assert!(!command_lab.can_add_row());

        command_lab.rows[1].command = "echo there".to_string();
        assert!(!command_lab.can_add_row());

        command_lab.rows[1].captured_commands = vec![command(0x0303, &[])];
        assert!(command_lab.can_add_row());
    }

    #[test]
    fn can_add_row_requires_the_last_row_to_have_a_recording() {
        let mut command_lab = CommandLab::new();
        command_lab.rows[0].command = "one".to_string();
        command_lab.rows[0].captured_commands = vec![command(0x0303, &[])];
        command_lab.rows.push(CommandLabRow {
            command: "two".to_string(),
            ..CommandLabRow::default()
        });

        assert!(!command_lab.can_add_row());

        command_lab.rows[1].captured_commands = vec![command(0x0303, &[])];
        assert!(command_lab.can_add_row());
    }

    #[test]
    fn removing_the_recording_row_stops_recording() {
        let mut command_lab = CommandLab::new();
        command_lab.rows[0].command = "one".to_string();
        command_lab.rows.push(CommandLabRow {
            command: "two".to_string(),
            ..CommandLabRow::default()
        });
        command_lab.recording_row_idx = Some(1);

        command_lab.remove_row(1);

        assert_eq!(command_lab.rows.len(), 1);
        assert_eq!(command_lab.recording_row_idx, None);
    }

    #[test]
    fn removing_a_row_before_the_recording_row_keeps_it_pointing_at_the_same_row() {
        let mut command_lab = CommandLab::new();
        command_lab.rows[0].command = "one".to_string();
        command_lab.rows.push(CommandLabRow {
            command: "two".to_string(),
            ..CommandLabRow::default()
        });
        command_lab.rows.push(CommandLabRow {
            command: "three".to_string(),
            ..CommandLabRow::default()
        });
        command_lab.recording_row_idx = Some(2);
        command_lab.remove_row(0);

        assert_eq!(command_lab.rows.len(), 2);
        assert_eq!(command_lab.recording_row_idx, Some(1));
    }

    #[test]
    fn removing_the_last_row_leaves_a_new_blank_row() {
        let mut command_lab = CommandLab::new();
        command_lab.rows[0].command = "one".to_string();

        command_lab.remove_row(0);

        assert_eq!(command_lab.rows.len(), 1);
        assert!(command_lab.rows[0].command.is_empty());
        assert!(command_lab.rows[0].captured_commands.is_empty());
        assert_eq!(command_lab.recording_row_idx, None);
    }

    fn command(code: u16, args: &[u8]) -> CapturedCommand {
        CapturedCommand {
            command: code,
            args: args.to_vec(),
        }
    }

    #[test]
    fn full_format_shows_command_and_args_truncated_after_three() {
        assert_eq!(
            format_command_full(&command(0x0303, &[0x01, 0x05, 0xFF])),
            "0x0303 01 05 FF"
        );
        assert_eq!(
            format_command_full(&command(0x0004, &[0x01, 0x02, 0x03, 0x04])),
            "0x0004 01 02 03 …"
        );
        assert_eq!(format_command_full(&command(0x0792, &[0x00])), "0x0792 00");
    }

    #[test]
    fn code_preview_shows_first_two_commands_without_prefix_and_remaining_count() {
        let commands = vec![
            command(0x0303, &[0x01, 0x05, 0xFF]),
            command(0x0792, &[0x00]),
            command(0x0303, &[0x01, 0x05, 0xFF]),
            command(0x0792, &[0x00]),
        ];

        assert_eq!(
            command_lab_code_preview(&commands),
            "0303, 0792 … 2 more"
        );
        assert_eq!(
            command_lab_code_preview(&commands[..3]),
            "0303, 0792 … 1 more"
        );
        assert_eq!(command_lab_code_preview(&commands[..2]), "0303, 0792");
        assert_eq!(command_lab_code_preview(&commands[..1]), "0303");
    }

    #[test]
    fn begin_capture_keeps_existing_captures_and_clears_the_failure_notice() {
        let mut command_lab = CommandLab::new();
        command_lab.rows[0].captured_commands = vec![command(0x0303, &[])];
        command_lab.rows[0].too_many_commands = true;

        command_lab.begin_capture(0);

        assert_eq!(command_lab.rows[0].captured_commands.len(), 1);
        assert!(!command_lab.rows[0].too_many_commands);
        assert_eq!(command_lab.recording_row_idx, Some(0));
    }

    #[test]
    fn captured_state_routes_to_the_recording_row_only() {
        let mut command_lab = CommandLab::new();
        command_lab.rows.push(CommandLabRow::default());
        command_lab.begin_capture(1);

        command_lab.set_captured_command_list(vec![command(0x0792, &[0x00])]);
        command_lab.apply_capture_result(
            CommandLabStatus::TooManyCommands,
            vec![command(0x0303, &[])],
        );

        assert!(command_lab.rows[0].captured_commands.is_empty());
        assert!(!command_lab.rows[0].too_many_commands);
        assert_eq!(command_lab.rows[1].captured_commands.len(), 1);
        assert!(command_lab.rows[1].too_many_commands);
    }

    #[test]
    fn populate_from_config_fills_matching_empty_rows_and_appends_new_ones() {
        let mut command_lab = CommandLab::new();
        command_lab.rows[0].command = "Brightness Up".to_string();
        command_lab.rows.push(CommandLabRow { command: "Fresh Capture".to_string(), captured_commands: vec![command(0x0004, &[])], too_many_commands: false, ..CommandLabRow::default() });

        command_lab.populate_from_config(HashMap::from([
            (
                "Brightness Up".to_string(),
                vec![command(0x0303, &[0x01, 0x05, 0xFF])],
            ),
            (
                "Battery Cycle".to_string(),
                vec![command(0x0712, &[])],
            ),
        ]));

        assert_eq!(command_lab.rows.len(), 3);
        assert_eq!(command_lab.rows[0].captured_commands.len(), 1);
        assert_eq!(command_lab.rows[1].captured_commands.len(), 1);
        assert_eq!(command_lab.rows[1].captured_commands[0].command, 0x0004);
        assert_eq!(command_lab.rows[2].command, "Battery Cycle");
        assert_eq!(command_lab.rows[2].captured_commands[0].command, 0x0712);
    }

    #[test]
    fn populate_from_config_drops_the_default_row_when_entries_exist() {
        let mut command_lab = CommandLab::new();
        assert_eq!(command_lab.rows.len(), 1);
        assert!(command_lab.rows[0].command.is_empty());

        command_lab.populate_from_config(HashMap::from([(
            "Underglow off".to_string(),
            vec![command(0x0303, &[])],
        )]));

        assert_eq!(command_lab.rows.len(), 1);
        assert_eq!(command_lab.rows[0].command, "Underglow off");
        assert_eq!(command_lab.rows[0].captured_commands.len(), 1);
    }

    #[test]
    fn populate_from_config_is_idempotent_across_refreshes() {
        let mut command_lab = CommandLab::new();
        let saved = HashMap::from([
            (
                "Underglow off".to_string(),
                vec![command(0x0303, &[])],
            ),
            (
                "Vapour chamber off".to_string(),
                vec![command(0x0303, &[])],
            ),
        ]);

        command_lab.populate_from_config(saved.clone());
        command_lab.populate_from_config(saved.clone());
        command_lab.populate_from_config(saved);

        assert_eq!(command_lab.rows.len(), 2);
    }

    #[test]
    fn populate_from_config_keeps_the_default_row_when_there_are_no_entries() {
        let mut command_lab = CommandLab::new();

        command_lab.populate_from_config(HashMap::new());

        assert_eq!(command_lab.rows.len(), 1);
        assert!(command_lab.rows[0].command.is_empty());
    }

    #[test]
    fn duplicate_name_blocks_saving_and_is_case_insensitive_on_whitespace() {
        let mut command_lab = CommandLab::new();
        command_lab.rows[0].command = "Underglow off".to_string();
        command_lab.rows.push(CommandLabRow {
            command: "  Underglow off ".to_string(),
            captured_commands: vec![command(0x0303, &[])],
            too_many_commands: false,
            ..CommandLabRow::default()
        });

        assert!(command_lab.row_name_is_duplicate(1));
        assert!(!command_lab.row_ready_to_save(1));

        command_lab.rows[1].command = "Unique".to_string();
        assert!(!command_lab.row_name_is_duplicate(1));
        assert!(command_lab.row_ready_to_save(1));
    }

    #[test]
    fn row_ready_to_save_requires_capture_unique_name_and_success() {
        let mut command_lab = CommandLab::new();
        command_lab.rows[0].command = "Ready".to_string();
        command_lab.rows[0].captured_commands = vec![command(0x0303, &[])];
        assert!(command_lab.row_ready_to_save(0));

        command_lab.rows[0].too_many_commands = true;
        assert!(!command_lab.row_ready_to_save(0));
        command_lab.rows[0].too_many_commands = false;

        command_lab.rows[0].captured_commands.clear();
        assert!(!command_lab.row_ready_to_save(0));
        command_lab.rows[0].captured_commands = vec![command(0x0303, &[])];

        command_lab.rows[0].command = "  ".to_string();
        assert!(!command_lab.row_ready_to_save(0));
    }

    #[test]
    fn failed_recapture_keeps_the_previous_commands() {
        let mut command_lab = CommandLab::new();
        let previous = vec![command(0x0303, &[])];
        command_lab.rows[0].captured_commands = previous.clone();
        command_lab.begin_capture(0);

        for status in [
            CommandLabStatus::Failed,
            CommandLabStatus::Cancelled,
            CommandLabStatus::Done,
        ] {
            command_lab.apply_capture_result(status, Vec::new());
            assert_eq!(
                command_lab.rows[0].captured_commands, previous,
                "{status:?} must keep the previous commands"
            );
        }
    }

    #[test]
    fn successful_recapture_replaces_the_commands() {
        let mut command_lab = CommandLab::new();
        command_lab.rows[0].captured_commands = vec![command(0x0303, &[])];
        command_lab.begin_capture(0);

        let fresh = vec![command(0x0792, &[0x00])];
        command_lab.apply_capture_result(CommandLabStatus::Done, fresh.clone());

        assert_eq!(command_lab.rows[0].captured_commands, fresh);
    }

    #[test]
    fn too_many_result_shows_notice_and_restores_previous_commands_after_duration() {
        let mut command_lab = CommandLab::new();
        let previous = vec![command(0x0303, &[0x01, 0x05, 0xFF])];
        command_lab.rows[0].captured_commands = previous.clone();
        command_lab.begin_capture(0);

        let too_many_list = vec![command(0x0303, &[]); 21];
        command_lab.apply_capture_result(CommandLabStatus::TooManyCommands, too_many_list.clone());

        assert!(command_lab.rows[0].too_many_commands);
        assert_eq!(command_lab.rows[0].captured_commands, too_many_list);

        let now = Instant::now() + COMMAND_LAB_TOO_MANY_NOTICE_DURATION;
        assert!(command_lab.rows[0].expire_too_many_notice(now));
        assert!(!command_lab.rows[0].too_many_commands);
        assert_eq!(command_lab.rows[0].captured_commands, previous);
        assert!(!command_lab.rows[0].expire_too_many_notice(now));
    }

    #[test]
    fn too_many_notice_does_not_expire_early() {
        let mut command_lab = CommandLab::new();
        command_lab.begin_capture(0);
        command_lab.apply_capture_result(
            CommandLabStatus::TooManyCommands,
            vec![command(0x0303, &[])],
        );

        let now = Instant::now() + COMMAND_LAB_TOO_MANY_NOTICE_DURATION
            - Duration::from_millis(1);
        assert!(!command_lab.rows[0].expire_too_many_notice(now));
        assert!(command_lab.rows[0].too_many_commands);
    }

    #[test]
    fn begin_capture_clears_a_pending_too_many_notice() {
        let mut command_lab = CommandLab::new();
        let previous = vec![command(0x0303, &[])];
        command_lab.rows[0].captured_commands = previous.clone();
        command_lab.begin_capture(0);
        command_lab.apply_capture_result(
            CommandLabStatus::TooManyCommands,
            vec![command(0x0303, &[])],
        );

        command_lab.begin_capture(0);

        assert!(!command_lab.rows[0].too_many_commands);
        assert_eq!(command_lab.rows[0].captured_commands, previous);
        let now = Instant::now() + COMMAND_LAB_TOO_MANY_NOTICE_DURATION;
        assert!(!command_lab.rows[0].expire_too_many_notice(now));
    }
}
