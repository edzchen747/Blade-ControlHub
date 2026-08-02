enum RazerKeyCaptureMessage {
    Captured { capture_id: u64, key_code: u8 },
}

impl RazerKeyCaptureMessage {
    fn capture_id(&self) -> u64 {
        match self {
            Self::Captured { capture_id, .. } => *capture_id,
        }
    }
}

fn run_razer_key_capture_worker(
    capture_id: u64,
    after_unix_ms: u64,
    cancel: Arc<AtomicBool>,
    tx: Sender<RazerKeyCaptureMessage>,
    ctx: egui::Context,
) {
    let mut after_sequence = loop {
        if cancel.load(Ordering::SeqCst) {
            return;
        }

        match client::begin_razer_key_capture(after_unix_ms) {
            Ok(after_sequence) => break after_sequence,
            Err(error) => {
                warn!(%error, "Razer key capture begin failed; retrying");
                thread::sleep(razer_key_capture_retry_interval());
            }
        }
    };

    while !cancel.load(Ordering::SeqCst) {
        match client::poll_captured_razer_key(after_sequence) {
            Ok(Some(event)) => {
                after_sequence = event.sequence;
                if event.key_code == 0 {
                    continue;
                }

                let _ = tx.send(RazerKeyCaptureMessage::Captured {
                    capture_id,
                    key_code: event.key_code,
                });
                ctx.request_repaint();
                return;
            }
            Ok(None) => thread::sleep(razer_key_capture_poll_interval()),
            Err(error) => {
                warn!(%error, "Razer key capture poll failed; retrying");
                thread::sleep(razer_key_capture_retry_interval());
            }
        }
    }
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.pace_frame();
        self.settings.with_settings(|settings| settings.ui(ctx));
        self.process_backend(ctx);
        self.update_window_icon(frame);
        self.sync_osd_suppression(ctx.input(|input| input.focused));
    }
}

fn razer_key_capture_poll_interval() -> Duration {
    Duration::from_millis(SETTINGS_KEY_LISTEN_INTERVAL_MS)
}

fn razer_key_capture_retry_interval() -> Duration {
    Duration::from_millis(100)
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

