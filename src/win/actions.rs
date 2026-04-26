use brightness::Brightness;
use windows::{
    Win32::Media::Audio::*,
    Win32::Media::Audio::Endpoints::*,
    Win32::System::Com::*,
};
use anyhow::Result;
use futures::stream::TryStreamExt;
use pollster;
use std::sync::OnceLock;

struct ComPtr<T>(pub T);
unsafe impl<T> Send for ComPtr<T> {}
unsafe impl<T> Sync for ComPtr<T> {}

static MIC_VOLUME_INTERFACE: OnceLock<Option<ComPtr<IAudioEndpointVolume>>> = OnceLock::new();
static OUT_VOLUME_INTERFACE: OnceLock<Option<ComPtr<IAudioEndpointVolume>>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[ repr(u8)]
pub enum AudioType {
    Speakers = 1,
    Mic = 2
}

pub fn adjust_brightness(change: i32) -> Result<()> {
    if change == 0 { return Ok(()); }

    // Logic: Spawn the async task and immediately return Ok
    std::thread::spawn(move || {
        pollster::block_on(async {
            let result = brightness::brightness_devices()
                .try_for_each(|mut dev| async move {
                    let current = dev.get().await?;
                    let new_value = (current as i32 + change).clamp(0, 100) as u32;
                    dev.set(new_value).await
                })
                .await;

            if let Err(e) = result {
                eprintln!("Brightness error: {}", e);
            }
        });
    });

    Ok(())
}

fn get_audio_interface(io: AudioType) -> Option<&'static IAudioEndpointVolume> {
    let audio_interface = match io {
        AudioType::Mic => &MIC_VOLUME_INTERFACE,
        AudioType::Speakers => &OUT_VOLUME_INTERFACE
    };
    let (dir, role) = match io {
       AudioType::Mic => (eCapture, eCommunications),
       AudioType::Speakers => (eRender, eConsole)
    }; 
    let wrapper = audio_interface.get_or_init(|| unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        let enumerator: IMMDeviceEnumerator = 
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
            
        let device = enumerator
            .GetDefaultAudioEndpoint(dir, role).ok()?;
            
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
            volume.GetMute()
                .map(|win_bool| win_bool.as_bool())
                .unwrap_or(false)
        }
    } else {
        false
    }
}

pub fn toggle_audio_mute(io: AudioType) -> anyhow::Result<()> {
    if let Some(volume) = get_audio_interface(io) {
        unsafe {
            let current_mute = volume.GetMute()?.as_bool();
            volume.SetMute(!current_mute, std::ptr::null())?;
        }
        Ok(())
    } else {
        Err(anyhow::anyhow!("Audio {:?} interface not available", io))
    }
}
