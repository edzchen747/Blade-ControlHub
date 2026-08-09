/// Command Lab recording state, owned by the main runtime.
///
/// The countdown worker lives in this process because the OSD overlay is
/// owned by the main runtime. The settings window only sends begin/cancel
/// requests and polls the recording status; it never drives the timer.
///
/// The worker starts the USBPcap capture first (this is where the
/// administrator-privileged filter open happens); the OSD countdown only
/// starts once the capture is actually running. When the capture cannot
/// start, the recording moves to `Failed` without showing the countdown.
use std::sync::mpsc;
use std::sync::Arc;

use crate::core::shared_state::COMMAND_LAB_CAPTURE_ACTIVE;
use crate::ipc::protocol::{CommandLabRecordingState, CommandLabStatus};
use crate::ui::icons::OsdIcon;
use crate::ui::osd_controller::{OsdController, OsdParams};
use crate::win::system::usbpcap::capture::CommandLabCapture;

pub const COMMAND_LAB_TOTAL_STEPS: usize = 5;
/// A capture that records more commands than this is discarded as a failure.
pub const COMMAND_LAB_MAX_CAPTURED_COMMANDS: u32 = 20;
const COMMAND_LAB_STEP_INTERVAL: Duration = Duration::from_secs(1);
const COMMAND_LAB_CANCEL_CHECK_INTERVAL: Duration = Duration::from_millis(50);

struct CommandLabRecording {
    state: CommandLabRecordingState,
    cancel: Option<Arc<AtomicBool>>,
    worker: Option<JoinHandle<()>>,
}

#[cfg(test)]
static COMMAND_LAB_TEST_OSD_SUPPRESSED: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
static COMMAND_LAB_TEST_CAPTURE_SUPPRESSED: AtomicBool = AtomicBool::new(false);

static COMMAND_LAB_RECORDING: Mutex<CommandLabRecording> =
    Mutex::new(CommandLabRecording::new());

fn command_lab_recording() -> MutexGuard<'static, CommandLabRecording> {
    COMMAND_LAB_RECORDING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl CommandLabRecording {
    const fn new() -> Self {
        Self {
            state: CommandLabRecordingState {
                status: CommandLabStatus::Idle,
                step: 0,
                captured_commands: 0,
                commands: Vec::new(),
            },
            cancel: None,
            worker: None,
        }
    }
}

/// Starts a Command Lab recording and blocks until the capture is actually
/// running (this is where a UAC elevation prompt is answered) or has failed
/// to start, returning the resulting state. The client therefore needs no
/// polling while the capture start is pending.
pub fn begin_command_lab_record() -> CommandLabRecordingState {
    cancel_active_worker();
    COMMAND_LAB_CAPTURE_ACTIVE.store(true, Ordering::SeqCst);

    let cancel = Arc::new(AtomicBool::new(false));
    let (started_tx, started_rx) = mpsc::channel::<CommandLabRecordingState>();
    {
        let mut recording = command_lab_recording();
        recording.cancel = Some(cancel.clone());
        recording.state = CommandLabRecordingState {
            status: CommandLabStatus::Idle,
            step: 0,
            captured_commands: 0,
            commands: Vec::new(),
        };
    }
    // No OSD yet: the worker only starts the countdown once the capture is
    // actually running.

    match thread::Builder::new()
        .name("blade-command-lab-record".to_string())
        .spawn(move || run_command_lab_worker(cancel, started_tx))
    {
        Ok(handle) => {
            command_lab_recording().worker = Some(handle);
            started_rx
                .recv()
                .unwrap_or_else(|_| CommandLabRecordingState {
                    status: CommandLabStatus::Failed,
                    step: 0,
                    captured_commands: 0,
                    commands: Vec::new(),
                })
        }
        Err(error) => {
            warn!(%error, "Failed to spawn Command Lab recording worker");
            command_lab_recording().cancel = None;
            COMMAND_LAB_CAPTURE_ACTIVE.store(false, Ordering::SeqCst);
            let state = CommandLabRecordingState {
                status: CommandLabStatus::Failed,
                step: 0,
                captured_commands: 0,
                commands: Vec::new(),
            };
            command_lab_recording().state = state.clone();
            state
        }
    }
}

pub fn cancel_command_lab_record() {
    cancel_active_worker();
}

pub fn poll_command_lab_recording() -> CommandLabRecordingState {
    command_lab_recording().state.clone()
}

/// Stops any active worker. The worker must not be joined while the state
/// mutex is held: the worker takes the same lock when it exits.
fn cancel_active_worker() {
    let worker = {
        let mut recording = command_lab_recording();
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
        warn!("Skipping join of current Command Lab recording worker");
        return;
    }
    if worker.join().is_err() {
        warn!("Command Lab recording worker panicked");
        COMMAND_LAB_CAPTURE_ACTIVE.store(false, Ordering::SeqCst);
    }
}

fn run_command_lab_worker(
    cancel: Arc<AtomicBool>,
    started_tx: mpsc::Sender<CommandLabRecordingState>,
) {
    let Some(capture) = start_command_lab_capture() else {
        let state = CommandLabRecordingState {
            status: CommandLabStatus::Failed,
            step: 0,
            captured_commands: 0,
            commands: Vec::new(),
        };
        command_lab_recording().state = state.clone();
        command_lab_recording().cancel = None;
        COMMAND_LAB_CAPTURE_ACTIVE.store(false, Ordering::SeqCst);
        let _ = started_tx.send(state);
        return;
    };

    // The capture is running: publish it and release the blocking begin so
    // the OSD countdown can start.
    let started = CommandLabRecordingState {
        status: CommandLabStatus::Recording,
        step: 0,
        captured_commands: capture.captured_count(),
        commands: Vec::new(),
    };
    command_lab_recording().state = started.clone();
    let _ = started_tx.send(started);

    let cancelled = run_command_lab_countdown(&cancel, COMMAND_LAB_STEP_INTERVAL, |step| {
        if cancel.load(Ordering::SeqCst) {
            return;
        }
        let state = CommandLabRecordingState {
            status: CommandLabStatus::Recording,
            step: step.min(COMMAND_LAB_TOTAL_STEPS) as u8,
            captured_commands: capture.captured_count(),
            commands: Vec::new(),
        };
        command_lab_recording().state = state.clone();
        show_command_lab_osd(&state);
    });

    let mut capture = capture;
    let captured_commands = capture.stop();
    let (status, step) = final_capture_status(
        cancelled || cancel.load(Ordering::SeqCst),
        captured_commands,
    );
    let commands = match status {
        CommandLabStatus::Done | CommandLabStatus::TooManyCommands => {
            capture.captured_commands()
        }
        _ => Vec::new(),
    };
    let state = CommandLabRecordingState {
        status,
        step,
        captured_commands,
        commands,
    };
    command_lab_recording().state = state.clone();
    show_command_lab_osd(&state);
    command_lab_recording().cancel = None;
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
        (CommandLabStatus::NoCommandsRecorded, COMMAND_LAB_TOTAL_STEPS as u8)
    } else {
        (CommandLabStatus::Done, COMMAND_LAB_TOTAL_STEPS as u8)
    }
}

/// Starts the USBPcap capture, or a dummy when tests suppress hardware use.
fn start_command_lab_capture() -> Option<CommandLabCapture> {
    #[cfg(test)]
    if COMMAND_LAB_TEST_CAPTURE_SUPPRESSED.load(Ordering::SeqCst) {
        return Some(CommandLabCapture::dummy());
    }
    CommandLabCapture::start()
}

/// Runs one progress step per `interval` until the total is reached or the
/// recording is cancelled. Each step publishes the next step index. Returns
/// whether the recording was cancelled.
fn run_command_lab_countdown(
    cancel: &AtomicBool,
    interval: Duration,
    mut on_step: impl FnMut(usize),
) -> bool {
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

/// Sleeps in small chunks so a cancelled recording exits promptly.
/// Returns whether the recording was cancelled during the sleep.
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

fn show_command_lab_osd(state: &CommandLabRecordingState) {
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
mod command_lab_tests {
    use super::*;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn reset_test_recording() {
        cancel_active_worker();
        command_lab_recording().state = CommandLabRecordingState {
            status: CommandLabStatus::Idle,
            step: 0,
            captured_commands: 0,
            commands: Vec::new(),
        };
    }

    fn with_test_recording<T>(test: impl FnOnce() -> T) -> T {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        COMMAND_LAB_TEST_OSD_SUPPRESSED.store(true, Ordering::SeqCst);
        COMMAND_LAB_TEST_CAPTURE_SUPPRESSED.store(true, Ordering::SeqCst);
        reset_test_recording();
        let result = test();
        COMMAND_LAB_TEST_OSD_SUPPRESSED.store(false, Ordering::SeqCst);
        COMMAND_LAB_TEST_CAPTURE_SUPPRESSED.store(false, Ordering::SeqCst);
        reset_test_recording();
        result
    }

    #[test]
    fn idle_recording_has_no_progress() {
        with_test_recording(|| {
            assert_eq!(
                poll_command_lab_recording(),
                CommandLabRecordingState {
                    status: CommandLabStatus::Idle,
                    step: 0,
                    captured_commands: 0,
                    commands: Vec::new(),
                }
            );
        });
    }

    #[test]
    fn countdown_reaches_done_with_full_progress() {
        with_test_recording(|| {
            let cancel = AtomicBool::new(false);
            let mut last_step = None;
            run_command_lab_countdown(&cancel, Duration::ZERO, |step| {
                last_step = Some(step);
            });

            assert_eq!(last_step, Some(COMMAND_LAB_TOTAL_STEPS));
        });
    }

    #[test]
    fn countdown_stops_early_when_cancelled() {
        with_test_recording(|| {
            let cancel = AtomicBool::new(false);
            let mut steps = Vec::new();
            let cancelled = run_command_lab_countdown(&cancel, Duration::ZERO, |step| {
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
                poll_command_lab_recording(),
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
    fn capture_gate_blocks_executor_until_the_worker_finishes() {
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
