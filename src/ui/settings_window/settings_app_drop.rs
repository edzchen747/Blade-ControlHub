impl Drop for SettingsApp {
    fn drop(&mut self) {
        if let Err(error) = client::set_settings_window_state(false, false) {
            warn!(%error, "Failed to clear settings window OSD suppression on close");
        }

        if let Some(cancel) = self.razer_key_capture_cancel.take() {
            cancel.store(true, Ordering::SeqCst);
            let _ = client::cancel_razer_key_capture();
        }

        if let Some(cancel) = self.command_lab_record_cancel.take() {
            cancel.store(true, Ordering::SeqCst);
            let _ = client::cancel_command_lab_record();
        }
    }
}
