/// How a held key keeps acting after the first press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Repeat {
    /// The overlay the key drives, if it has one, as an `OsdIcon::kind_key`.
    /// It lets the overlay tell a fresh press from the key still being held.
    pub osd_kind: Option<u8>,
}

pub struct KeyEventAction<'a> {
    action: Box<dyn Fn() + Send + Sync>,
    condition: Vec<Source<'a>>,
    repeat: Option<Repeat>,
}
impl<'a> KeyEventAction<'a> {
    pub fn new(action: Box<dyn Fn() + Send + Sync>, condition: Vec<Source<'a>>) -> Self {
        Self {
            action,
            condition,
            repeat: None,
        }
    }

    /// Marks the action as one that should keep running while its key is held,
    /// even when another key has since been pressed. Windows only auto-repeats
    /// the most recent key, so without this a second held key silences the first.
    pub fn repeating(mut self, osd_kind: Option<u8>) -> Self {
        self.repeat = Some(Repeat { osd_kind });
        self
    }

    pub fn repeat(&self) -> Option<Repeat> {
        self.repeat
    }

    pub fn execute(&self) -> bool {
        let conditions_met = self.condition.iter().all(|check| check.eval());
        if conditions_met {
            (self.action)();
        }
        conditions_met
    }
}
