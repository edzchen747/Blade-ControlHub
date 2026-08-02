fn device_worker_unavailable() -> AppError {
    AppError::Internal("Device worker is unavailable".to_string())
}

// ── DeviceController Trait Implementation ─────────────────────────────────────

impl crate::hal::DeviceController for DeviceHandle {
    fn initialize(&self, notify_startup: bool) {
        DeviceHandle::initialize(self, notify_startup);
    }

    fn sleep(&self) -> AppResult<bool> {
        DeviceHandle::sleep(self)
    }

    fn shutdown(&self) -> AppResult<bool> {
        DeviceHandle::shutdown(self)
    }

    fn get_pid(&self) -> AppResult<u16> {
        DeviceHandle::get_pid(self)
    }

    fn cycle_perf_mode(&self) {
        DeviceHandle::cycle_perf_mode(self);
    }

    fn cycle_rgb_mode(&self) {
        DeviceHandle::cycle_rgb_mode(self);
    }

    fn cycle_refresh_rate(&self) {
        DeviceHandle::cycle_refresh_rate(self);
    }

    fn cycle_battery_limit(&self) {
        DeviceHandle::cycle_battery_limit(self);
    }

    fn toggle_vc(&self) {
        DeviceHandle::toggle_vc(self);
    }

    fn keyboard_light_up(&self) {
        DeviceHandle::keyboard_light_up(self);
    }

    fn keyboard_light_down(&self) {
        DeviceHandle::keyboard_light_down(self);
    }

    fn adjust_screen_brightness(&self, change: i8) {
        DeviceHandle::adjust_screen_brightness(self, change);
    }

    fn set_lid_logo(&self, mode: LidLogoMode) {
        DeviceHandle::set_lid_logo(self, mode);
    }

    fn persist_config(&self) {
        DeviceHandle::persist_config(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn closed_handle() -> DeviceHandle {
        let (tx, rx) = mpsc::channel::<DeviceCmd>();
        drop(rx);
        let (urgent_sender, urgent_rx) = mpsc::channel::<DeviceCmd>();
        drop(urgent_rx);
        DeviceHandle {
            sender: tx,
            urgent_sender,
        }
    }

    #[test]
    fn query_returns_internal_when_worker_channel_is_closed() {
        let result = closed_handle().get_pid();

        assert!(matches!(
            result,
            Err(AppError::Internal(message)) if message.contains("Device worker is unavailable")
        ));
    }

    #[test]
    fn fire_and_forget_command_does_not_panic_when_worker_channel_is_closed() {
        let handle = closed_handle();

        handle.initialize(false);
    }

    #[test]
    fn stop_device_channel_monitor_clears_running_flag() {
        DEVICE_CHANNEL_MONITOR_RUNNING.store(true, Ordering::SeqCst);

        stop_device_channel_monitor();

        assert!(!DEVICE_CHANNEL_MONITOR_RUNNING.load(Ordering::SeqCst));
    }

    #[test]
    fn join_device_channel_monitor_thread_drains_handle() {
        *device_channel_monitor_thread() = Some(thread::spawn(|| {}));

        join_device_channel_monitor_thread();

        assert!(device_channel_monitor_thread().is_none());
    }

    #[test]
    fn query_returns_internal_when_worker_drops_response_sender() {
        let (tx, rx) = mpsc::channel::<DeviceCmd>();
        let (urgent_sender, urgent_rx) = mpsc::channel::<DeviceCmd>();
        drop(urgent_rx);
        let handle = DeviceHandle {
            sender: tx,
            urgent_sender,
        };
        let worker = thread::spawn(move || {
            let _ = rx.recv();
        });

        let result = handle.get_pid();
        worker.join().expect("test worker must exit");

        assert!(matches!(
            result,
            Err(AppError::Internal(message)) if message.contains("Device worker is unavailable")
        ));
    }
}
