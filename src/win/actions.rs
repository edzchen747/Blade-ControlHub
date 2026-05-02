use brightness::Brightness;
use futures::stream::StreamExt;
use pollster;
use std::sync::OnceLock;
use windows::{Win32::Media::Audio::Endpoints::*, Win32::Media::Audio::*, Win32::System::Com::*};
use winreg::RegKey;
use winreg::enums::*;

use crate::ui::{app_events::AppEvent, tray_app::tray_app};

struct ComPtr<T>(pub T);
unsafe impl<T> Send for ComPtr<T> {}
unsafe impl<T> Sync for ComPtr<T> {}

static MIC_VOLUME_INTERFACE: OnceLock<Option<ComPtr<IAudioEndpointVolume>>> = OnceLock::new();
static OUT_VOLUME_INTERFACE: OnceLock<Option<ComPtr<IAudioEndpointVolume>>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AudioType {
    Speakers = 1,
    Mic = 2,
}

pub fn get_screen_brightness() -> u8 {
    pollster::block_on(async {
        let mut dev_stream = brightness::brightness_devices();
        match dev_stream.next().await {
            Some(Ok(dev)) => dev.get().await.unwrap_or(100) as u8,
            _ => 0,
        }
    })
}

fn get_audio_interface(io: AudioType) -> Option<&'static IAudioEndpointVolume> {
    let audio_interface = match io {
        AudioType::Mic => &MIC_VOLUME_INTERFACE,
        AudioType::Speakers => &OUT_VOLUME_INTERFACE,
    };
    let (dir, role) = match io {
        AudioType::Mic => (eCapture, eCommunications),
        AudioType::Speakers => (eRender, eConsole),
    };
    let wrapper = audio_interface.get_or_init(|| unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;

        let device = enumerator.GetDefaultAudioEndpoint(dir, role).ok()?;

        let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None).ok()?;

        Some(ComPtr(volume))
    });

    // Use Option::as_ref to go from &Option<T> to Option<&T>
    // Then map to get the inner COM interface
    wrapper.as_ref().map(|w| &w.0)
}

pub fn is_audio_muted(io: AudioType) -> bool {
    if let Some(volume) = get_audio_interface(io) {
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

pub fn toggle_audio_mute(io: AudioType) {
    if let Some(volume) = get_audio_interface(io) {
        unsafe {
            if let Ok(current_mute) = volume.GetMute() {
                current_mute.as_bool();
                if let Err(err) = volume.SetMute(!current_mute, std::ptr::null()) {
                    println!("Error setting {:?} endpoint mute", io);
                    println!("{:?}", err);
                }
                match io {
                    AudioType::Mic => {
                        tray_app().send(AppEvent::MicMute(bool::from(!current_mute)));
                    }
                    _ => (),
                };
            } else {
                println!("Error getting {:?} endpoint mute", io);
            }
        };
    } else {
        println!("Audio {:?} interface not available", io);
    };
}

pub fn get_trackpad_state() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = r"Software\Microsoft\Windows\CurrentVersion\PrecisionTouchPad\Status";

    if let Ok(key) = hkcu.open_subkey(path) {
        match key.get_value::<u32, _>("Enabled") {
            Ok(enabled) => enabled != 0,
            Err(err) => {
                println!("{}", err);
                true
            }
        }
    } else {
        println!("Could not find PrecisionTouchPad registry key.");
        true
    }
}
