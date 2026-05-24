use crate::error::AppError;
use crate::razer::config::AppConfig;
use crate::razer::device_handle::DeviceCmd;
use crate::razer::enums::RGBEffect;
use crate::razer::handlers::{
    AudioHandler, BatteryHandler, DisplayHandler, KeyboardHandler, PerformanceHandler,
};
use crate::razer::protocol::command;
use crate::ui::app::app;
use crate::ui::app_events::OsdEvent;
use crate::utils::persist::PersistBuffer;
use crate::win::display::ambient::AmbientEffect;
use crate::win::display::brightness::BrightnessWorker;
use crate::win::display::refresh_rate::DisplayManager;
use librazer::device::Device;
use std::sync::mpsc::Receiver;
use std::time::Instant;
use tracing::{info, instrument};

pub struct Executer<'a> {
    device: &'a Device,
    app_config: &'a mut AppConfig,
    persist_buffer: PersistBuffer,
    rx: Receiver<DeviceCmd>,
    brightness_worker: BrightnessWorker,
    display_manager: DisplayManager,
    refresh_cycle_timeout: Instant,
    battery_cycle_timeout: Instant,
}

impl<'a> Executer<'a> {
    pub fn new(
        device: &'a Device,
        app_config: &'a mut AppConfig,
        persist_buffer: PersistBuffer,
        rx: Receiver<DeviceCmd>,
    ) -> Result<Self, AppError> {
        let display_manager = DisplayManager::new().ok_or(AppError::DisplayNotFound)?;
        Ok(Self {
            device,
            app_config,
            persist_buffer,
            rx,
            brightness_worker: BrightnessWorker::new(),
            display_manager,
            refresh_cycle_timeout: Instant::now(),
            battery_cycle_timeout: Instant::now(),
        })
    }
    pub fn process_commands(&mut self) {
        while let Ok(cmd) = self.rx.recv() {
            self.dispatch(cmd);
        }
        info!("All DeviceHandle senders dropped; device worker thread exiting");
    }
    fn kb(&mut self) -> KeyboardHandler<'_> {
        KeyboardHandler::new(self.device, self.app_config, &self.persist_buffer)
    }
    fn perf(&mut self) -> PerformanceHandler<'_> {
        PerformanceHandler::new(self.device, self.app_config, &self.persist_buffer)
    }
    fn display(&mut self) -> DisplayHandler<'_> {
        DisplayHandler::new(
            self.device,
            self.app_config,
            &self.persist_buffer,
            &mut self.display_manager,
            &mut self.refresh_cycle_timeout,
        )
    }
    fn battery(&mut self) -> BatteryHandler<'_> {
        BatteryHandler::new(
            self.device,
            self.app_config,
            &self.persist_buffer,
            &mut self.battery_cycle_timeout,
        )
    }
    fn dispatch(&mut self, cmd: DeviceCmd) {
        match cmd {
            DeviceCmd::InitializeDevice(n) => self.initialize(n),
            DeviceCmd::SleepDevice => self.sleep(),
            DeviceCmd::Shutdown(tx) => {
                let _ = tx.send(self.shutdown());
            }
            DeviceCmd::AdjustKeyboardLight(up) => self.kb().adjust_keyboard_light(up),
            DeviceCmd::CycleRGBMode => self.kb().cycle_rgb_mode(),
            DeviceCmd::ToggleUnderGlow => self.kb().toggle_under_glow(),
            DeviceCmd::SetKeyboardColor(r, g, b) => self.kb().set_keyboard_color(r, g, b),
            DeviceCmd::SetLidLogo(mode) => self.kb().set_lid_logo(mode),
            DeviceCmd::CyclePerfMode => self.perf().cycle_perf_mode(),
            DeviceCmd::AdjustScreenBrightness(change) => {
                self.brightness_worker.adjust_screen_brightness(change);
            }
            DeviceCmd::CycleRefreshRate => self.display().cycle_refresh_rate(),
            DeviceCmd::SetMuteIndicator(io, muted) => {
                AudioHandler::new(self.device).set_mute_indicator(io, muted);
            }
            DeviceCmd::CycleBatteryLimit => self.battery().cycle_battery_limit(),
            DeviceCmd::GetPID(tx) => {
                let _ = tx.send(self.device.info.pid);
            }
            DeviceCmd::GetPerfMode(tx) => {
                let _ = tx.send(self.perf().get_perf_mode());
            }
            DeviceCmd::GetDefaultMultimediaKeys(tx) => {
                let _ = tx.send(self.kb().get_default_multimedia_keys());
            }
            DeviceCmd::ToggleDefaultMultimediaKeys(tx) => {
                let _ = tx.send(self.kb().toggle_default_multimedia_keys());
            }
            DeviceCmd::GetConfig(tx) => {
                let _ = tx.send(self.app_config.clone());
            }
            DeviceCmd::PersistConfig => {
                crate::config::persist_config(self.app_config, &self.persist_buffer);
            }
        }
    }
    #[instrument(skip(self), fields(notify_startup))]
    fn initialize(&mut self, notify_startup: bool) {
        PersistBuffer::disable();
        app().send(OsdEvent::EnableOSD(false).into());
        let mut state = self.app_config.read();
        self.brightness_worker
            .set_screen_brightness(state.screen_lvl);
        self.kb().keyboard_control(true);
        self.kb().enable_multimedia_keys();
        self.kb().set_rgb_effect(state.rgb_effect.value());
        self.kb().enable_under_glow(state.vc_lvl);
        self.kb().set_keyboard_brightness(state.key_lvl);
        self.perf().set_perf_mode(state.perf_mode.value());
        let limit = self.app_config.battery_limit.value();
        self.battery().set_battery_limit(limit);
        if self.app_config.default_multimedia_keys {
            self.kb().enable_multimedia_keys();
        } else {
            self.kb().restore_fn_keys();
        }
        app().send(OsdEvent::EnableOSD(true).into());
        if notify_startup {
            app().send(OsdEvent::Startup.into());
        }
        if state.screen_refresh != 0 {
            self.display().set_refresh_rate(state.screen_refresh);
        }
        PersistBuffer::enable();
    }
    fn sleep(&mut self) {
        self.kb().keyboard_control(false);
        let _ = command(self.device, 0x030a, &[5, 0], None);
        AmbientEffect::stop();
        let _ = command(self.device, 0x0303, &[1, 5, 0], None);
        let _ = command(self.device, 0x0300, &[1, 38, 0], None);
        let _ = command(self.device, 0x0303, &[1, 38, 0], None);
        let _ = command(self.device, 0x0d02, &[1, 0, 6, 0], None);
    }

    fn shutdown(&mut self) -> bool {
        self.kb().restore_fn_keys();
        self.kb().keyboard_control(false);
        self.kb().set_rgb_effect(RGBEffect::Cycle);
        true
    }
}
