use crate::ipc::protocol::CommandLabRecordingState;

enum CommandLabRecordMessage {
    State {
        record_id: u64,
        state: CommandLabRecordingState,
    },
    Finished { record_id: u64 },
}

fn run_command_lab_record_worker(
    record_id: u64,
    cancel: Arc<AtomicBool>,
    tx: Sender<CommandLabRecordMessage>,
    ctx: egui::Context,
) {
    let started = loop {
        if cancel.load(Ordering::SeqCst) {
            return;
        }

        match client::begin_command_lab_record() {
            Ok(state) => break state,
            Err(error) => {
                warn!(%error, "Command Lab record begin failed; retrying");
                thread::sleep(command_lab_record_retry_interval());
            }
        }
    };
    if command_lab_status_is_terminal(started.status) {
        let _ = tx.send(CommandLabRecordMessage::State { record_id, state: started });
        let _ = tx.send(CommandLabRecordMessage::Finished { record_id });
        ctx.request_repaint();
        return;
    }

    while !cancel.load(Ordering::SeqCst) {
        match client::poll_command_lab_recording() {
            Ok(state) => {
                let terminal = command_lab_status_is_terminal(state.status);
                let _ = tx.send(CommandLabRecordMessage::State { record_id, state });
                ctx.request_repaint();
                if terminal {
                    let _ = tx.send(CommandLabRecordMessage::Finished { record_id });
                    ctx.request_repaint();
                    return;
                }
                thread::sleep(command_lab_record_poll_interval());
            }
            Err(error) => {
                warn!(%error, "Command Lab record poll failed; retrying");
                thread::sleep(command_lab_record_retry_interval());
            }
        }
    }
}

fn command_lab_status_is_terminal(status: crate::ipc::protocol::CommandLabStatus) -> bool {
    matches!(
        status,
        crate::ipc::protocol::CommandLabStatus::Done
            | crate::ipc::protocol::CommandLabStatus::Cancelled
            | crate::ipc::protocol::CommandLabStatus::Failed
            | crate::ipc::protocol::CommandLabStatus::TooManyCommands
    )
}

fn command_lab_record_poll_interval() -> Duration {
    Duration::from_millis(SETTINGS_KEY_LISTEN_INTERVAL_MS)
}

fn command_lab_record_retry_interval() -> Duration {
    Duration::from_millis(100)
}
