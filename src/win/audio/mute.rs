use super::{AudioType, get_endpoint};
use crate::ui::{app::app, app_events::OsdEvent};
use tracing::warn;

/// Returns whether the given audio endpoint is currently muted.
pub fn is_audio_muted(io: AudioType) -> bool {
    if let Some(volume) = get_endpoint(io) {
        unsafe {
            volume
                .GetMute()
                .map(|win_bool| win_bool.as_bool())
                .unwrap_or(false)
        }
    } else {
        false
    }
}

/// Toggles mute on the given audio endpoint and notifies the tray app for mic changes.
pub fn toggle_audio_mute(io: AudioType) {
    if let Some(volume) = get_endpoint(io) {
        unsafe {
            if let Ok(current_mute) = volume.GetMute() {
                if let Err(err) = volume.SetMute(!current_mute, std::ptr::null()) {
                    warn!(endpoint = ?io, error = ?err, "Failed to set endpoint mute");
                }
                if io == AudioType::Mic {
                    app().send(OsdEvent::MicMute(bool::from(!current_mute)).into());
                }
            } else {
                warn!(endpoint = ?io, "Failed to read endpoint mute state");
            }
        };
    } else {
        warn!(endpoint = ?io, "Audio interface not available");
    };
}
