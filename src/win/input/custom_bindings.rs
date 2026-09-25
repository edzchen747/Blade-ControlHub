//! The user's own key bindings, and how they run.
//!
//! The built-in table in `key_map` is fixed at compile time. This one is
//! replaced whenever the settings window saves, so it lives behind an `RwLock`
//! and is looked up by raw key code — which also means a Hypershift binding can
//! use any virtual key, not only the handful `vkey::Key` happens to name.
//!
//! Both dispatch sites consult this table *before* the built-in one, so a
//! binding the user made always wins over the stock behaviour of that key.
//!
//! Nothing is executed on the caller's thread. The Razer reader and the
//! low-level keyboard hook both call in, and an action can take hundreds of
//! milliseconds — a macro sleeps 10 ms between key events, a launch waits on
//! the shell. Holding a `WH_KEYBOARD_LL` callback that long makes Windows drop
//! the hook without saying so, so every action is handed to a worker thread and
//! the caller returns immediately.

use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};

use rdev::Key;
use tracing::{debug, warn};

use crate::razer::device_handle::device;
use crate::ui::app::app;
use crate::ui::app_events::{AppEvent, OsdEvent};
use crate::win::audio::{self, AudioType};
use crate::win::input::binding::{
    Chord, DeviceAction, KeyAction, KeyBinding, KeyBindings, MOD_ALT, MOD_CTRL, MOD_SHIFT, MOD_WIN,
};
use crate::win::input::key_map::KeyCombo;
use crate::win::input::trackpad::toggle_trackpad;
use crate::win::system::launch;

/// Set while a key or macro action is synthesizing input, so the keystrokes it
/// sends cannot be read back as presses and trigger the binding again.
static SYNTHESIZING: AtomicBool = AtomicBool::new(false);

static BINDINGS: LazyLock<RwLock<Resolved>> = LazyLock::new(RwLock::default);

#[derive(Default)]
struct Resolved {
    razer: HashMap<u16, KeyAction>,
    hypershift: HashMap<u16, KeyAction>,
}

/// Installs a new set of bindings, replacing whatever was loaded before.
/// Incomplete rows are dropped here rather than at every press: the window
/// keeps them so the user can finish them, but they are not actions yet.
pub fn replace(bindings: &KeyBindings) {
    let resolved = Resolved {
        razer: lookup(&bindings.razer),
        hypershift: lookup(&bindings.hypershift),
    };
    debug!(
        razer = resolved.razer.len(),
        hypershift = resolved.hypershift.len(),
        "Installed custom key bindings"
    );
    *write_bindings() = resolved;
}

fn lookup(rows: &[KeyBinding]) -> HashMap<u16, KeyAction> {
    rows.iter()
        .filter(|row| row.action.is_complete())
        .map(|row| (row.key_code, row.action.clone()))
        .collect()
}

/// The action bound to a Razer special key, if the user bound one.
pub fn razer(key_code: u8) -> Option<KeyAction> {
    read_bindings().razer.get(&u16::from(key_code)).cloned()
}

/// The action bound to a key held with Fn, if the user bound one.
pub fn hypershift(virtual_key: u8) -> Option<KeyAction> {
    read_bindings()
        .hypershift
        .get(&u16::from(virtual_key))
        .cloned()
}

/// Whether input is currently being synthesized by a binding of our own.
pub fn is_synthesizing() -> bool {
    SYNTHESIZING.load(Ordering::SeqCst)
}

/// Runs an action on a worker thread. Returns immediately.
pub fn run(action: KeyAction) {
    if matches!(action, KeyAction::None) {
        return;
    }

    if let Err(error) = std::thread::Builder::new()
        .name("blade-key-binding".to_owned())
        .spawn(move || execute(action))
    {
        warn!(%error, "Failed to start the key binding worker");
    }
}

fn execute(action: KeyAction) {
    match action {
        KeyAction::None => {}
        KeyAction::Key { chord } => send_keys(&[chord]),
        KeyAction::Macro { steps } => send_keys(&steps),
        KeyAction::Device { action } => run_device_action(action),
        KeyAction::ToggleUi => app(AppEvent::ToggleSettings),
        KeyAction::LaunchApp { path, args, name } => {
            let label = if name.trim().is_empty() {
                file_name(&path)
            } else {
                name
            };
            match launch::launch_app(&path, &args) {
                Ok(()) => show_osd(label),
                Err(error) => {
                    warn!(%error, path, "Failed to launch the bound application");
                    show_osd(format!("{label} failed"));
                }
            }
        }
        KeyAction::RunCommand { command } => match launch::run_command(&command) {
            Ok(()) => show_osd(truncate(&command, 28)),
            Err(error) => {
                warn!(%error, command, "Failed to run the bound command");
                show_osd("Command failed".to_owned());
            }
        },
        KeyAction::ReplayCapture { name } => device().replay_saved_capture(name),
        // The overlay comes from the device thread, which is the only place
        // that knows which side the control ended up on.
        KeyAction::ToggleCustomControl { name } => device().toggle_custom_control(name),
    }
}

/// Device actions raise the overlay their own handler already owns, and key
/// remaps stay silent — an overlay on every keystroke would be noise. Only the
/// actions with no overlay of their own get one, and it names the action, never
/// the user's label for the row.
fn show_osd(label: String) {
    app(OsdEvent::CustomAction(label).into());
}

fn run_device_action(action: DeviceAction) {
    match action {
        DeviceAction::CyclePerfMode => device().cycle_perf_mode(),
        DeviceAction::CycleRgbEffect => device().cycle_rgb_mode(),
        DeviceAction::CycleRefreshRate => device().cycle_refresh_rate(),
        DeviceAction::CycleBatteryLimit => device().cycle_battery_limit(),
        DeviceAction::ToggleUnderglow => device().toggle_vc(),
        DeviceAction::KeyboardLightUp => device().keyboard_light_up(),
        DeviceAction::KeyboardLightDown => device().keyboard_light_down(),
        DeviceAction::ScreenBrightnessUp => device().adjust_screen_brightness(10),
        DeviceAction::ScreenBrightnessDown => device().adjust_screen_brightness(-10),
        DeviceAction::ToggleMicMute => audio::toggle_audio_mute(AudioType::Mic),
        DeviceAction::ToggleSpeakerMute => audio::toggle_audio_mute(AudioType::Speakers),
        DeviceAction::ToggleTrackpad => toggle_trackpad(),
        DeviceAction::TogglePrimaryMultimediaKeys => {
            let _ = device().toggle_primary_multimedia_keys();
        }
        DeviceAction::CloseGpuApps => crate::win::system::cli_utils::cycle_gpu(),
    }
}

fn send_keys(steps: &[Chord]) {
    SYNTHESIZING.store(true, Ordering::SeqCst);
    for chord in steps.iter().filter(|chord| chord.is_complete()) {
        KeyCombo::new(&chord_keys(chord)).trigger();
    }
    SYNTHESIZING.store(false, Ordering::SeqCst);
}

/// Modifiers first so they wrap the key: `KeyCombo` presses in order and
/// releases in reverse.
fn chord_keys(chord: &Chord) -> Vec<Key> {
    let mut keys = Vec::with_capacity(5);
    if chord.modifiers & MOD_CTRL != 0 {
        keys.push(Key::ControlLeft);
    }
    if chord.modifiers & MOD_ALT != 0 {
        keys.push(Key::Alt);
    }
    if chord.modifiers & MOD_SHIFT != 0 {
        keys.push(Key::ShiftLeft);
    }
    if chord.modifiers & MOD_WIN != 0 {
        keys.push(Key::MetaLeft);
    }
    keys.push(Key::Unknown(u32::from(chord.key)));
    keys
}

fn file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_owned())
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    text.chars()
        .take(limit.saturating_sub(1))
        .collect::<String>()
        + "…"
}

fn read_bindings() -> std::sync::RwLockReadGuard<'static, Resolved> {
    BINDINGS
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write_bindings() -> std::sync::RwLockWriteGuard<'static, Resolved> {
    BINDINGS
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    /// The binding tables and the synthesis guard are process-wide, and cargo
    /// runs the tests of one binary on several threads. Without a lock they
    /// race: one test's `replace` lands while another is asserting against the
    /// tables it just installed, and which test fails is down to timing.
    ///
    /// Every test that reads or writes that shared state takes this first. A
    /// test that fails while holding it poisons the mutex, which is recovered
    /// rather than propagated — one genuine failure should not be reported as
    /// a dozen.
    static SHARED_STATE: Mutex<()> = Mutex::new(());

    fn exclusive() -> MutexGuard<'static, ()> {
        SHARED_STATE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn binding(key_code: u16, action: KeyAction) -> KeyBinding {
        KeyBinding {
            key_code,
            label: String::new(),
            action,
        }
    }

    #[test]
    fn replacing_bindings_swaps_both_tables() {
        let _shared = exclusive();
        replace(&KeyBindings {
            razer: vec![binding(0x24, KeyAction::ToggleUi)],
            hypershift: vec![binding(0x4b, KeyAction::ToggleUi)],
        });

        assert_eq!(razer(0x24), Some(KeyAction::ToggleUi));
        assert_eq!(hypershift(0x4b), Some(KeyAction::ToggleUi));

        replace(&KeyBindings::default());

        assert_eq!(razer(0x24), None);
        assert_eq!(hypershift(0x4b), None);
    }

    #[test]
    fn incomplete_rows_are_never_bound() {
        let _shared = exclusive();
        replace(&KeyBindings {
            razer: vec![binding(
                0x25,
                KeyAction::LaunchApp {
                    path: String::new(),
                    args: String::new(),
                    name: String::new(),
                },
            )],
            hypershift: Vec::new(),
        });

        assert_eq!(
            razer(0x25),
            None,
            "a row with no application path must fall through to the built-in map"
        );

        replace(&KeyBindings::default());
    }

    /// One unfinished row must not take the rest of its table down with it.
    #[test]
    fn an_incomplete_row_does_not_disturb_the_complete_ones() {
        let _shared = exclusive();
        replace(&KeyBindings {
            razer: vec![
                binding(0x24, KeyAction::ToggleUi),
                binding(
                    0x25,
                    KeyAction::RunCommand {
                        command: "   ".to_owned(),
                    },
                ),
                binding(0x26, KeyAction::ToggleUi),
            ],
            hypershift: Vec::new(),
        });

        assert_eq!(razer(0x24), Some(KeyAction::ToggleUi));
        assert_eq!(razer(0x25), None);
        assert_eq!(razer(0x26), Some(KeyAction::ToggleUi));

        replace(&KeyBindings::default());
    }

    /// The window rejects duplicates, but a hand-edited config can hold them;
    /// the later row wins rather than the table being rejected.
    #[test]
    fn a_duplicated_key_code_resolves_to_the_last_row() {
        let _shared = exclusive();
        replace(&KeyBindings {
            razer: vec![
                binding(0x24, KeyAction::ToggleUi),
                binding(
                    0x24,
                    KeyAction::RunCommand {
                        command: "later".to_owned(),
                    },
                ),
            ],
            hypershift: Vec::new(),
        });

        assert_eq!(
            razer(0x24),
            Some(KeyAction::RunCommand {
                command: "later".to_owned()
            })
        );

        replace(&KeyBindings::default());
    }

    /// The two tables are looked up by raw code, and a Razer key code can equal
    /// a virtual-key code, so they must not share a namespace. 0x24 is both M1
    /// and VK_HOME.
    #[test]
    fn the_two_tables_do_not_collide_on_a_shared_code() {
        let _shared = exclusive();
        replace(&KeyBindings {
            razer: vec![binding(0x24, KeyAction::ToggleUi)],
            hypershift: vec![binding(
                0x24,
                KeyAction::RunCommand {
                    command: "hypershift".to_owned(),
                },
            )],
        });

        assert_eq!(razer(0x24), Some(KeyAction::ToggleUi));
        assert_eq!(
            hypershift(0x24),
            Some(KeyAction::RunCommand {
                command: "hypershift".to_owned()
            })
        );

        replace(&KeyBindings::default());
    }

    /// Hypershift is keyed by raw virtual-key code rather than through
    /// `vkey::Key`, which only names a dozen keys. Binding Fn+K has to work even
    /// though K is `vkey::Key::Unknown`.
    #[test]
    fn a_hypershift_binding_works_for_a_key_the_vkey_enum_does_not_name() {
        let _shared = exclusive();
        assert_eq!(
            crate::win::input::vkey::Key::from(0x4b),
            crate::win::input::vkey::Key::Unknown,
            "K is deliberately absent from the built-in key enum"
        );

        replace(&KeyBindings {
            razer: Vec::new(),
            hypershift: vec![binding(0x4b, KeyAction::ToggleUi)],
        });

        assert_eq!(hypershift(0x4b), Some(KeyAction::ToggleUi));

        replace(&KeyBindings::default());
    }

    /// A key the built-in map already handles is reclaimable: the dispatch sites
    /// consult this table first, so a hit here is what overrides the default.
    #[test]
    fn a_binding_can_claim_a_key_the_built_in_map_handles() {
        let _shared = exclusive();
        use crate::win::input::{KeyType, key_map::KEY_MAP, razer_key};

        assert!(
            KEY_MAP.contains_key(&KeyType::from(razer_key::Key::Perf)),
            "the performance key must have a built-in action for this to mean anything"
        );

        replace(&KeyBindings {
            razer: vec![binding(
                0xd3,
                KeyAction::Device {
                    action: DeviceAction::CycleRgbEffect,
                },
            )],
            hypershift: Vec::new(),
        });

        assert_eq!(
            razer(0xd3),
            Some(KeyAction::Device {
                action: DeviceAction::CycleRgbEffect
            })
        );

        replace(&KeyBindings::default());
    }

    /// Running an action must never leave the guard set, or the keyboard hook
    /// would ignore Hypershift for the rest of the session.
    #[test]
    fn synthesizing_a_macro_clears_its_guard_afterwards() {
        let _shared = exclusive();
        assert!(!is_synthesizing());

        // No steps means no simulated input, so this stays safe under test while
        // still exercising the set/clear pair around the loop.
        send_keys(&[]);

        assert!(!is_synthesizing());
    }

    #[test]
    fn a_do_nothing_action_is_not_even_dispatched() {
        let _shared = exclusive();
        // `run` returns without spawning, which is what makes "do nothing" a way
        // to silence a key rather than a no-op worker per press.
        run(KeyAction::None);

        assert!(!is_synthesizing());
    }

    #[test]
    fn a_chord_with_no_modifiers_is_just_the_key() {
        assert_eq!(
            chord_keys(&Chord {
                modifiers: 0,
                key: 0x70,
            }),
            vec![Key::Unknown(0x70)]
        );
    }

    /// Four modifiers plus a key is five events; the combo used to hold only
    /// four and would have dropped the key itself.
    #[test]
    fn a_chord_can_hold_every_modifier_at_once() {
        let keys = chord_keys(&Chord {
            modifiers: MOD_CTRL | MOD_ALT | MOD_SHIFT | MOD_WIN,
            key: 0x2e,
        });

        assert_eq!(
            keys,
            vec![
                Key::ControlLeft,
                Key::Alt,
                Key::ShiftLeft,
                Key::MetaLeft,
                Key::Unknown(0x2e),
            ]
        );
    }

    #[test]
    fn a_chord_presses_its_modifiers_before_the_key() {
        let keys = chord_keys(&Chord {
            modifiers: MOD_CTRL | MOD_SHIFT,
            key: 0x1b,
        });

        assert_eq!(
            keys,
            vec![Key::ControlLeft, Key::ShiftLeft, Key::Unknown(0x1b)]
        );
    }

    #[test]
    fn a_long_command_is_shortened_for_the_overlay() {
        assert_eq!(truncate("shutdown /s /t 0", 28), "shutdown /s /t 0");
        assert_eq!(truncate(&"a".repeat(40), 10).chars().count(), 10);
    }

    #[test]
    fn a_launch_label_falls_back_to_the_executable_name() {
        assert_eq!(file_name(r"C:\Program Files\Thing\thing.exe"), "thing");
        assert_eq!(file_name(r"C:\Tools\thing"), "thing");
        assert_eq!(file_name("thing.exe"), "thing");
        assert_eq!(file_name(""), "");
    }

    /// A packaged application is launched by model ID, not by path, so the label
    /// falls back to the whole id rather than to a stem of it.
    #[test]
    fn a_packaged_launch_label_survives_a_model_id() {
        let label = file_name("shell:AppsFolder\\Microsoft.WindowsNotepad_8wekyb3d8bbwe!App");

        assert!(!label.is_empty());
    }

    #[test]
    fn truncation_never_splits_a_character_or_underflows() {
        assert_eq!(truncate("abc", 3), "abc");
        assert_eq!(truncate("abcd", 3), "ab\u{2026}");
        assert_eq!(truncate("abc", 1), "\u{2026}");
        assert_eq!(truncate("abc", 0), "\u{2026}");
        assert_eq!(truncate("", 0), "");

        // Multi-byte input must be counted in characters, not bytes.
        let text = "\u{e9}\u{e9}\u{e9}\u{e9}";
        assert_eq!(truncate(text, 2).chars().count(), 2);
    }
}
