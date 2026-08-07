/// Data structures for Command Lab state.
///
/// Rows are plain text commands; the recording lifecycle is a UI-local
/// concern that is mirrored by the runtime countdown through IPC.

#[derive(Default, Clone)]
pub struct CommandLabRow {
    pub command: String,
}

/// The Command Lab editing state, holding the growing row list and which
/// row (if any) is currently recording.
#[derive(Default)]
pub struct CommandLab {
    pub rows: Vec<CommandLabRow>,
    pub recording_row_idx: Option<usize>,
}

impl CommandLab {
    pub fn new() -> Self {
        Self {
            rows: vec![CommandLabRow::default()],
            recording_row_idx: None,
        }
    }

    pub fn recording_row_idx(&self) -> Option<usize> {
        self.recording_row_idx
    }

    pub fn set_recording_row_idx(&mut self, idx: Option<usize>) {
        self.recording_row_idx = idx;
    }

    pub fn is_recording(&self) -> bool {
        self.recording_row_idx.is_some()
    }

    /// Whether a new row can be added: no active recording and every
    /// existing row has a command entered, mirroring the key mapper tabs.
    pub fn can_add_row(&self) -> bool {
        self.recording_row_idx.is_none() && self.rows.iter().all(|row| !row.command.is_empty())
    }

    pub fn add_row(&mut self) {
        self.rows.push(CommandLabRow::default());
    }

    /// Removes a row, clearing the recording row if it was removed.
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
    }
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
        assert!(command_lab.can_add_row());
    }

    #[test]
    fn removing_the_recording_row_stops_recording() {
        let mut command_lab = CommandLab::new();
        command_lab.rows[0].command = "one".to_string();
        command_lab.rows.push(CommandLabRow {
            command: "two".to_string(),
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
        });
        command_lab.rows.push(CommandLabRow {
            command: "three".to_string(),
        });
        command_lab.recording_row_idx = Some(2);

        command_lab.remove_row(0);

        assert_eq!(command_lab.rows.len(), 2);
        assert_eq!(command_lab.recording_row_idx, Some(1));
    }
}
