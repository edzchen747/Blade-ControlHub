//! Command Lab recording, owned by the runtime.
//!
//! The countdown lives here rather than in the window because the OSD is the
//! primary feedback surface: the user is inside Synapse while recording, not
//! looking at ControlHub. The window mirrors the same countdown from the
//! states pushed here.
//!
//! The worker starts the USBPcap capture first — that is where the
//! administrator-privileged filter open, and any UAC prompt, happens — and only
//! then starts the countdown. A capture that cannot start goes straight to
//! `Failed` without showing a countdown that was never recording anything.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tracing::{debug, warn};

use crate::core::shared_state::COMMAND_LAB_CAPTURE_ACTIVE;
use crate::ui::icons::OsdIcon;
use crate::ui::osd_controller::{OsdController, OsdParams};
use crate::win::system::usbpcap::capture::{
    CapturedCommand, CommandLabCapture, reset_self_emitted, without_self_emitted,
};

/// Event name the window listens on for recording progress.
pub const COMMAND_LAB_EVENT: &str = "command-lab";

pub const COMMAND_LAB_TOTAL_STEPS: usize = 5;
/// A capture that records more commands than this is discarded as a failure:
/// the extra traffic means something other than the setting under test was
/// also talking to the device.
pub const COMMAND_LAB_MAX_CAPTURED_COMMANDS: u32 = 20;
const COMMAND_LAB_STEP_INTERVAL: Duration = Duration::from_secs(1);
const COMMAND_LAB_CANCEL_CHECK_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandLabStatus {
    Idle,
    Recording,
    Done,
    Cancelled,
    Failed,
    TooManyCommands,
    NoCommandsRecorded,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandLabRecordingState {
    pub status: CommandLabStatus,
    pub step: u8,
    pub captured_commands: u32,
    /// The parsed commands of a finished capture; empty while recording and
    /// for cancelled or never-started recordings.
    pub commands: Vec<CapturedCommand>,
}

impl CommandLabRecordingState {
    const fn idle() -> Self {
        Self {
            status: CommandLabStatus::Idle,
            step: 0,
            captured_commands: 0,
            commands: Vec::new(),
        }
    }

    fn failed() -> Self {
        Self {
            status: CommandLabStatus::Failed,
            ..Self::idle()
        }
    }
}

struct CommandLabRecording {
    state: CommandLabRecordingState,
    cancel: Option<Arc<AtomicBool>>,
    worker: Option<JoinHandle<()>>,
}

impl CommandLabRecording {
    const fn new() -> Self {
        Self {
            state: CommandLabRecordingState::idle(),
            cancel: None,
            worker: None,
        }
    }
}

static COMMAND_LAB_RECORDING: Mutex<CommandLabRecording> = Mutex::new(CommandLabRecording::new());

#[cfg(test)]
static COMMAND_LAB_TEST_OSD_SUPPRESSED: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
static COMMAND_LAB_TEST_CAPTURE_SUPPRESSED: AtomicBool = AtomicBool::new(false);

fn recording() -> MutexGuard<'static, CommandLabRecording> {
    COMMAND_LAB_RECORDING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Starts a recording and blocks until the capture is actually running — this
/// is where a UAC prompt is answered — or has failed to start, returning the
/// resulting state. Every later state arrives as a [`COMMAND_LAB_EVENT`].
pub fn begin_command_lab_record() -> CommandLabRecordingState {
    cancel_active_worker();
    reset_self_emitted();
    COMMAND_LAB_CAPTURE_ACTIVE.store(true, Ordering::SeqCst);

    let cancel = Arc::new(AtomicBool::new(false));
    let (started_tx, started_rx) = mpsc::channel::<CommandLabRecordingState>();
    {
        let mut recording = recording();
        recording.cancel = Some(cancel.clone());
        recording.state = CommandLabRecordingState::idle();
    }

    match thread::Builder::new()
        .name("blade-command-lab-record".to_string())
        .spawn(move || run_worker(cancel, started_tx))
    {
        Ok(handle) => {
            recording().worker = Some(handle);
            started_rx
                .recv()
                .unwrap_or_else(|_| CommandLabRecordingState::failed())
        }
        Err(error) => {
            warn!(%error, "Failed to spawn the Command Lab recording worker");
            recording().cancel = None;
            COMMAND_LAB_CAPTURE_ACTIVE.store(false, Ordering::SeqCst);
            let state = CommandLabRecordingState::failed();
            recording().state = state.clone();
            state
        }
    }
}

pub fn cancel_command_lab_record() {
    cancel_active_worker();
}

/// Stops any recording during shutdown, so the USBPcap filter is released.
pub fn stop_command_lab_recording() {
    cancel_active_worker();
}

pub fn command_lab_state() -> CommandLabRecordingState {
    recording().state.clone()
}

/// Stops any active worker. The worker must not be joined while the state
/// mutex is held: it takes the same lock when it exits.
fn cancel_active_worker() {
    let worker = {
        let mut recording = recording();
        if let Some(cancel) = recording.cancel.take() {
            cancel.store(true, Ordering::SeqCst);
        }
        recording.worker.take()
    };
    if let Some(worker) = worker {
        join_worker(worker);
    }
}

fn join_worker(worker: JoinHandle<()>) {
    if worker.thread().id() == thread::current().id() {
        warn!("Skipping join of the current Command Lab recording worker");
        return;
    }
    if worker.join().is_err() {
        warn!("Command Lab recording worker panicked");
        COMMAND_LAB_CAPTURE_ACTIVE.store(false, Ordering::SeqCst);
    }
}

fn run_worker(cancel: Arc<AtomicBool>, started_tx: mpsc::Sender<CommandLabRecordingState>) {
    let Some(capture) = start_capture() else {
        let state = CommandLabRecordingState::failed();
        recording().state = state.clone();
        recording().cancel = None;
        COMMAND_LAB_CAPTURE_ACTIVE.store(false, Ordering::SeqCst);
        publish(&state);
        let _ = started_tx.send(state);
        return;
    };

    let started = CommandLabRecordingState {
        status: CommandLabStatus::Recording,
        step: 0,
        captured_commands: capture.captured_count(),
        commands: Vec::new(),
    };
    recording().state = started.clone();
    let _ = started_tx.send(started);

    let cancelled = run_countdown(&cancel, COMMAND_LAB_STEP_INTERVAL, |step| {
        if cancel.load(Ordering::SeqCst) {
            return;
        }
        let state = CommandLabRecordingState {
            status: CommandLabStatus::Recording,
            step: step.min(COMMAND_LAB_TOTAL_STEPS) as u8,
            captured_commands: capture.captured_count(),
            commands: Vec::new(),
        };
        recording().state = state.clone();
        show_osd(&state);
        publish(&state);
    });

    let mut capture = capture;
    let raw_count = capture.stop();
    // Drop our own writes before judging the capture: counting them would push
    // an otherwise fine recording over the too-many-commands limit, and
    // replaying them would just repeat what ControlHub already does.
    let commands = without_self_emitted(capture.captured_commands());
    let captured_commands = commands.len() as u32;
    if raw_count != captured_commands {
        debug!(
            raw_count,
            captured_commands, "Discarded ControlHub's own commands from the capture"
        );
    }
    let (status, step) =
        final_capture_status(cancelled || cancel.load(Ordering::SeqCst), captured_commands);
    let commands = match status {
        CommandLabStatus::Done | CommandLabStatus::TooManyCommands => commands,
        _ => Vec::new(),
    };
    let state = CommandLabRecordingState {
        status,
        step,
        captured_commands,
        commands,
    };
    recording().state = state.clone();
    show_osd(&state);
    publish(&state);
    recording().cancel = None;
    COMMAND_LAB_CAPTURE_ACTIVE.store(false, Ordering::SeqCst);
}

/// Maps a finished capture to its published status and step. Cancellations
/// keep full progress; a capture that recorded more commands than a row can
/// hold is reported as a failure with zero progress.
fn final_capture_status(cancelled: bool, captured_commands: u32) -> (CommandLabStatus, u8) {
    if cancelled {
        (CommandLabStatus::Cancelled, COMMAND_LAB_TOTAL_STEPS as u8)
    } else if captured_commands > COMMAND_LAB_MAX_CAPTURED_COMMANDS {
        (CommandLabStatus::TooManyCommands, 0)
    } else if captured_commands == 0 {
        (
            CommandLabStatus::NoCommandsRecorded,
            COMMAND_LAB_TOTAL_STEPS as u8,
        )
    } else {
        (CommandLabStatus::Done, COMMAND_LAB_TOTAL_STEPS as u8)
    }
}

/// Starts the USBPcap capture, or a dummy when tests suppress hardware use.
fn start_capture() -> Option<CommandLabCapture> {
    #[cfg(test)]
    if COMMAND_LAB_TEST_CAPTURE_SUPPRESSED.load(Ordering::SeqCst) {
        return Some(CommandLabCapture::dummy());
    }
    CommandLabCapture::start()
}

/// Runs one progress step per `interval` until the total is reached or the
/// recording is cancelled. Returns whether it was cancelled.
fn run_countdown(cancel: &AtomicBool, interval: Duration, mut on_step: impl FnMut(usize)) -> bool {
    for step in 0..=COMMAND_LAB_TOTAL_STEPS {
        if cancel.load(Ordering::SeqCst) {
            return true;
        }

        on_step(step);

        if step < COMMAND_LAB_TOTAL_STEPS && sleep_interruptible(cancel, interval) {
            return true;
        }
    }
    false
}

/// Sleeps in small chunks so a cancelled recording exits promptly. Returns
/// whether the recording was cancelled during the sleep.
fn sleep_interruptible(cancel: &AtomicBool, duration: Duration) -> bool {
    let mut remaining = duration;
    while remaining > COMMAND_LAB_CANCEL_CHECK_INTERVAL {
        thread::sleep(COMMAND_LAB_CANCEL_CHECK_INTERVAL);
        remaining -= COMMAND_LAB_CANCEL_CHECK_INTERVAL;
        if cancel.load(Ordering::SeqCst) {
            return true;
        }
    }
    thread::sleep(remaining);
    cancel.load(Ordering::SeqCst)
}

fn publish(state: &CommandLabRecordingState) {
    let Some(handle) = super::app_handle() else {
        return;
    };
    if let Err(error) = handle.emit(COMMAND_LAB_EVENT, state) {
        warn!(%error, "Failed to publish Command Lab progress to the window");
    }
}

fn show_osd(state: &CommandLabRecordingState) {
    #[cfg(test)]
    if COMMAND_LAB_TEST_OSD_SUPPRESSED.load(Ordering::SeqCst) {
        return;
    }
    let (label, active_steps) = match state.status {
        CommandLabStatus::Recording => ("Recording", state.step as usize),
        CommandLabStatus::Done => ("Done", state.step as usize),
        CommandLabStatus::TooManyCommands => ("Failed", 0),
        CommandLabStatus::NoCommandsRecorded => ("No commands", state.step as usize),
        CommandLabStatus::Cancelled => ("Cancelled", state.step as usize),
        CommandLabStatus::Idle | CommandLabStatus::Failed => return,
    };
    OsdController::show(OsdParams {
        label: label.to_string(),
        total_steps: COMMAND_LAB_TOTAL_STEPS,
        active_steps,
        icon: Some(OsdIcon::CommandLab),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn reset() {
        cancel_active_worker();
        recording().state = CommandLabRecordingState::idle();
    }

    fn with_test_recording<T>(test: impl FnOnce() -> T) -> T {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        COMMAND_LAB_TEST_OSD_SUPPRESSED.store(true, Ordering::SeqCst);
        COMMAND_LAB_TEST_CAPTURE_SUPPRESSED.store(true, Ordering::SeqCst);
        reset();
        let result = test();
        COMMAND_LAB_TEST_OSD_SUPPRESSED.store(false, Ordering::SeqCst);
        COMMAND_LAB_TEST_CAPTURE_SUPPRESSED.store(false, Ordering::SeqCst);
        reset();
        result
    }

    #[test]
    fn idle_recording_has_no_progress() {
        with_test_recording(|| {
            assert_eq!(command_lab_state(), CommandLabRecordingState::idle());
        });
    }

    #[test]
    fn countdown_reaches_done_with_full_progress() {
        with_test_recording(|| {
            let cancel = AtomicBool::new(false);
            let mut last_step = None;
            run_countdown(&cancel, Duration::ZERO, |step| last_step = Some(step));

            assert_eq!(last_step, Some(COMMAND_LAB_TOTAL_STEPS));
        });
    }

    #[test]
    fn countdown_stops_early_when_cancelled() {
        with_test_recording(|| {
            let cancel = AtomicBool::new(false);
            let mut steps = Vec::new();
            let cancelled = run_countdown(&cancel, Duration::ZERO, |step| {
                steps.push(step);
                if step == 2 {
                    cancel.store(true, Ordering::SeqCst);
                }
            });

            assert!(cancelled);
            assert_eq!(steps, vec![0, 1, 2]);
        });
    }

    #[test]
    fn finished_capture_reports_done_with_full_progress() {
        assert_eq!(
            final_capture_status(false, COMMAND_LAB_MAX_CAPTURED_COMMANDS),
            (CommandLabStatus::Done, COMMAND_LAB_TOTAL_STEPS as u8)
        );
    }

    #[test]
    fn capture_over_the_limit_reports_failed_with_zero_progress() {
        assert_eq!(
            final_capture_status(false, COMMAND_LAB_MAX_CAPTURED_COMMANDS + 1),
            (CommandLabStatus::TooManyCommands, 0)
        );
    }

    #[test]
    fn empty_capture_reports_no_commands_with_full_progress() {
        assert_eq!(
            final_capture_status(false, 0),
            (
                CommandLabStatus::NoCommandsRecorded,
                COMMAND_LAB_TOTAL_STEPS as u8
            )
        );
    }

    #[test]
    fn cancelled_capture_stays_cancelled_even_over_the_limit() {
        assert_eq!(
            final_capture_status(true, COMMAND_LAB_MAX_CAPTURED_COMMANDS + 1),
            (CommandLabStatus::Cancelled, COMMAND_LAB_TOTAL_STEPS as u8)
        );
    }

    #[test]
    fn begin_blocks_until_recording_starts_and_cancel_publishes_cancelled_state() {
        with_test_recording(|| {
            let started = begin_command_lab_record();

            assert_eq!(
                started,
                CommandLabRecordingState {
                    status: CommandLabStatus::Recording,
                    step: 0,
                    captured_commands: 0,
                    commands: Vec::new(),
                }
            );

            cancel_command_lab_record();

            assert_eq!(
                command_lab_state(),
                CommandLabRecordingState {
                    status: CommandLabStatus::Cancelled,
                    step: COMMAND_LAB_TOTAL_STEPS as u8,
                    captured_commands: 0,
                    commands: Vec::new(),
                }
            );
        });
    }

    #[test]
    fn capture_gate_blocks_the_executor_until_the_worker_finishes() {
        with_test_recording(|| {
            assert!(!COMMAND_LAB_CAPTURE_ACTIVE.load(Ordering::SeqCst));
            let started = begin_command_lab_record();
            assert_eq!(started.status, CommandLabStatus::Recording);
            assert!(COMMAND_LAB_CAPTURE_ACTIVE.load(Ordering::SeqCst));
            cancel_command_lab_record();
            assert!(!COMMAND_LAB_CAPTURE_ACTIVE.load(Ordering::SeqCst));
        });
    }
}
