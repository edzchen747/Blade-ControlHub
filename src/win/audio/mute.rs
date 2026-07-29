use super::{AudioType, with_endpoint};
use crate::ui::{app::app, app_events::OsdEvent};
use tracing::warn;

/// Returns whether the given audio endpoint is currently muted.
pub fn is_audio_muted(io: AudioType) -> bool {
    match with_endpoint(io, |volume| unsafe { volume.GetMute() }) {
        Ok(mute) => mute.as_bool(),
        Err(error) => {
            warn!(endpoint = ?io, ?error, "Failed to read endpoint mute state");
            false
        }
    }
}

/// Toggles mute on the given audio endpoint and notifies the tray app for mic changes.
pub fn toggle_audio_mute(io: AudioType) {
    let result = with_endpoint(io, |volume| unsafe {
        let current_mute = volume.GetMute()?.as_bool();
        let new_mute = !current_mute;
        volume.SetMute(new_mute, std::ptr::null())?;
        Ok(new_mute)
    });

    match result {
        Ok(new_mute) if io == AudioType::Mic => {
            app(OsdEvent::MicMute(new_mute).into());
        }
        Ok(_) => {}
        Err(error) => warn!(endpoint = ?io, ?error, "Failed to toggle endpoint mute"),
    }
}
