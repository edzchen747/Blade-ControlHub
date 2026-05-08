use super::{AudioType, get_endpoint};
use crate::ui::{app_events::AppEvent, tray_app::tray_app};

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
                current_mute.as_bool();
                if let Err(err) = volume.SetMute(!current_mute, std::ptr::null()) {
                    println!("Error setting {:?} endpoint mute: {:?}", io, err);
                }
                if io == AudioType::Mic {
                    tray_app().send(AppEvent::MicMute(bool::from(!current_mute)));
                }
            } else {
                println!("Error getting {:?} endpoint mute", io);
            }
        };
    } else {
        println!("Audio {:?} interface not available", io);
    };
}
