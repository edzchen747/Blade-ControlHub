impl Drop for SettingsApp {
    fn drop(&mut self) {
        if let Some(cancel) = self.razer_key_capture_cancel.take() {
            cancel.store(true, Ordering::SeqCst);
            let _ = client::cancel_razer_key_capture();
        }
    }
}

