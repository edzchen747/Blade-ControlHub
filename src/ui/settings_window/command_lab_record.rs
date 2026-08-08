/// Command Lab recording worker.
///
/// Starts the runtime countdown over IPC, then polls until the recording
/// reaches a terminal state so the UI can restore the Record button. Every
/// polled state is forwarded so the UI can show the live captured-command
/// count. Mirrors the Razer key capture worker: transient IPC failures are
/// retried until the flow is cancelled, and pipe delays must not stall the UI.
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
    loop {
        if cancel.load(Ordering::SeqCst) {
            return;
        }

        match client::begin_command_lab_record() {
            Ok(()) => break,
            Err(error) => {
                warn!(%error, "Command Lab record begin failed; retrying");
                thread::sleep(command_lab_record_retry_interval());
            }
        }
    }

    while !cancel.load(Ordering::SeqCst) {
        match client::poll_command_lab_recording() {
            Ok(state) => {
                let _ = tx.send(CommandLabRecordMessage::State { record_id, state });
                ctx.request_repaint();
                if state.status == crate::ipc::protocol::CommandLabStatus::Done
                    || state.status == crate::ipc::protocol::CommandLabStatus::Cancelled
                    || state.status == crate::ipc::protocol::CommandLabStatus::Failed
                {
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

fn command_lab_record_poll_interval() -> Duration {
    Duration::from_millis(SETTINGS_KEY_LISTEN_INTERVAL_MS)
}

fn command_lab_record_retry_interval() -> Duration {
    Duration::from_millis(100)
}
