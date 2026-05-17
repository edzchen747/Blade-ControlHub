use librazer::device::Device;

use crate::{razer::protocol::command, win::audio::AudioType};

/// Audio handler.
///
/// Accepts a reference to the device. All logic is copied exactly from Executer.
pub struct AudioHandler<'a> {
    device: &'a Device,
}

impl<'a> AudioHandler<'a> {
    pub fn new(device: &'a Device) -> Self {
        Self { device }
    }

    pub fn set_mute_indicator(&self, io: AudioType, muted: bool) {
        let _ = command(self.device, 0x1804, &[0, io as u8, muted as u8], None);
    }
}
