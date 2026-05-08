use crate::{
    razer::device_handle::device,
    win::{
        audio::{self, AudioType},
        display::{
            brightness::{SCREEN_ADJUSTING, SCREEN_TARGET_LVL},
            screen_query::get_screen_brightness,
        },
    },
};
use std::sync::atomic::Ordering;
use std::{thread, time::Duration};

pub fn spawn_detect_external_updates_thread() {
    thread::spawn(|| {
        ExternalChangeMonitor::new(100, 1000).run_loop();
    });
}

struct ExternalChangeMonitor {
    fast_interval_ms: u64,
    slow_interval_ms: u64,
    mic_muted: bool,
    speakers_muted: bool,
    screen_brightness: u8,
}

impl ExternalChangeMonitor {
    pub fn new(fast: u64, slow: u64) -> Self {
        Self {
            fast_interval_ms: fast,
            slow_interval_ms: slow,
            mic_muted: audio::is_audio_muted(AudioType::Mic),
            speakers_muted: audio::is_audio_muted(AudioType::Speakers),
            screen_brightness: get_screen_brightness(),
        }
    }

    pub fn run_loop(&mut self) {
        let mut interval = 0;
        loop {
            let curr_mic_muted = audio::is_audio_muted(AudioType::Mic);
            if curr_mic_muted != self.mic_muted {
                device().set_mic_mute_indicator(curr_mic_muted);
            };
            self.mic_muted = curr_mic_muted;

            let curr_speakers_muted = audio::is_audio_muted(AudioType::Speakers);
            if curr_speakers_muted != self.speakers_muted {
                device().set_speakers_mute_indicator(curr_speakers_muted);
            };
            self.speakers_muted = curr_speakers_muted;

            interval += self.fast_interval_ms;

            if interval >= self.slow_interval_ms {
                if SCREEN_ADJUSTING.load(Ordering::SeqCst) == 0 {
                    interval = 0;
                    let curr_screen_brightness = get_screen_brightness();
                    if self.screen_brightness != curr_screen_brightness {
                        SCREEN_TARGET_LVL.store(curr_screen_brightness, Ordering::SeqCst);
                        device().persist_config();
                        self.screen_brightness = curr_screen_brightness;
                        println!("Detect brightness = {}", curr_screen_brightness);
                    }
                }
            }
            thread::sleep(Duration::from_millis(self.fast_interval_ms));
        }
    }
}
