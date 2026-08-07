/// Command Lab recording state, owned by the main runtime.
///
/// The countdown worker lives in this process because the OSD overlay is
/// owned by the main runtime. The settings window only sends begin/cancel
/// requests and polls the recording status; it never drives the timer.
use std::sync::Arc;

use crate::ipc::protocol::{CommandLabRecordingState, CommandLabStatus};
use crate::ui::icons::OsdIcon;
use crate::ui::osd_controller::{OsdController, OsdParams};

pub const COMMAND_LAB_TOTAL_STEPS: usize = 5;
const COMMAND_LAB_STEP_INTERVAL: Duration = Duration::from_secs(1);
const COMMAND_LAB_CANCEL_CHECK_INTERVAL: Duration = Duration::from_millis(50);

struct CommandLabRecording {
    state: CommandLabRecordingState,
    cancel: Option<Arc<AtomicBool>>,
    worker: Option<JoinHandle<()>>,
}

#[cfg(test)]
static COMMAND_LAB_TEST_OSD_SUPPRESSED: AtomicBool = AtomicBool::new(false);

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
            },
            cancel: None,
            worker: None,
        }
    }
}

pub fn begin_command_lab_record() {
    cancel_active_worker();

    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut recording = command_lab_recording();
        recording.cancel = Some(cancel.clone());
        recording.state = CommandLabRecordingState {
            status: CommandLabStatus::Recording,
            step: 0,
        };
    }
    show_command_lab_osd(&command_lab_recording().state);

    match thread::Builder::new()
        .name("blade-command-lab-record".to_string())
        .spawn(move || run_command_lab_worker(cancel))
    {
        Ok(handle) => command_lab_recording().worker = Some(handle),
        Err(error) => {
            warn!(%error, "Failed to spawn Command Lab recording worker");
            command_lab_recording().cancel = None;
            command_lab_recording().state = CommandLabRecordingState {
                status: CommandLabStatus::Idle,
                step: 0,
            };
        }
    }
}

pub fn cancel_command_lab_record() {
    cancel_active_worker();
    command_lab_recording().state = CommandLabRecordingState {
        status: CommandLabStatus::Cancelled,
        step: 0,
    };
    show_command_lab_osd(&command_lab_recording().state);
}

pub fn poll_command_lab_recording() -> CommandLabRecordingState {
    command_lab_recording().state
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
    }
}

fn run_command_lab_worker(cancel: Arc<AtomicBool>) {
    run_command_lab_countdown(&cancel, COMMAND_LAB_STEP_INTERVAL, |state| {
        if cancel.load(Ordering::SeqCst) {
            return;
        }
        command_lab_recording().state = state;
        show_command_lab_osd(&state);
    });
    command_lab_recording().cancel = None;
}

/// Runs one progress step per `interval` until the total is reached or the
/// recording is cancelled. Each step publishes the next recording state.
fn run_command_lab_countdown(
    cancel: &AtomicBool,
    interval: Duration,
    mut on_step: impl FnMut(CommandLabRecordingState),
) {
    for step in 0..=COMMAND_LAB_TOTAL_STEPS {
        if cancel.load(Ordering::SeqCst) {
            return;
        }

        on_step(step_state(step));

        if step < COMMAND_LAB_TOTAL_STEPS && sleep_interruptible(cancel, interval) {
            return;
        }
    }
}

fn step_state(step: usize) -> CommandLabRecordingState {
    CommandLabRecordingState {
        status: if step >= COMMAND_LAB_TOTAL_STEPS {
            CommandLabStatus::Done
        } else {
            CommandLabStatus::Recording
        },
        step: step.min(COMMAND_LAB_TOTAL_STEPS) as u8,
    }
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
    let label = match state.status {
        CommandLabStatus::Recording => "Recording",
        CommandLabStatus::Done => "Done",
        CommandLabStatus::Cancelled => "Cancelled",
        CommandLabStatus::Idle => return,
    };
    OsdController::show(OsdParams {
        label: label.to_string(),
        total_steps: COMMAND_LAB_TOTAL_STEPS,
        active_steps: state.step as usize,
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
        };
    }

    fn with_test_recording<T>(test: impl FnOnce() -> T) -> T {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        COMMAND_LAB_TEST_OSD_SUPPRESSED.store(true, Ordering::SeqCst);
        reset_test_recording();
        let result = test();
        COMMAND_LAB_TEST_OSD_SUPPRESSED.store(false, Ordering::SeqCst);
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
                }
            );
        });
    }

    #[test]
    fn countdown_reaches_done_with_full_progress() {
        with_test_recording(|| {
            let cancel = AtomicBool::new(false);
            let mut last_state = None;
            run_command_lab_countdown(&cancel, Duration::ZERO, |state| {
                last_state = Some(state);
            });

            assert_eq!(
                last_state,
                Some(CommandLabRecordingState {
                    status: CommandLabStatus::Done,
                    step: COMMAND_LAB_TOTAL_STEPS as u8,
                })
            );
        });
    }

    #[test]
    fn countdown_stops_early_when_cancelled() {
        with_test_recording(|| {
            let cancel = AtomicBool::new(false);
            let mut states = Vec::new();
            run_command_lab_countdown(&cancel, Duration::ZERO, |state| {
                states.push(state);
                if state.step == 2 {
                    cancel.store(true, Ordering::SeqCst);
                }
            });

            assert_eq!(states.len(), 3);
            assert_eq!(
                states.last(),
                Some(&CommandLabRecordingState {
                    status: CommandLabStatus::Recording,
                    step: 2,
                })
            );
        });
    }

    #[test]
    fn begin_resets_state_to_recording_at_zero_and_cancel_empties_it() {
        with_test_recording(|| {
            begin_command_lab_record();

            assert_eq!(
                poll_command_lab_recording(),
                CommandLabRecordingState {
                    status: CommandLabStatus::Recording,
                    step: 0,
                }
            );

            cancel_command_lab_record();

            assert_eq!(
                poll_command_lab_recording(),
                CommandLabRecordingState {
                    status: CommandLabStatus::Cancelled,
                    step: 0,
                }
            );
        });
    }
}
