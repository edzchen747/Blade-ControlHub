//! What the detected model can do, for the parts of the app that never hold
//! the device.
//!
//! The descriptor in `librazer` is the single source of truth, but it only
//! reaches the device worker thread. The keyboard hook, the binding table and
//! the settings snapshot all need to know whether a control exists before they
//! offer it, so the descriptor's answer is recorded here once the model has
//! been identified and read from wherever it is needed.
//!
//! An unrecognised model falls back to a descriptor with no features at all, so
//! everything gated here stays off rather than being driven blind.

use std::sync::atomic::{AtomicBool, Ordering};

use librazer::descriptor::Descriptor;
use librazer::feature;

/// Whether this model has the illuminated vent under the chassis.
///
/// Public because the built-in key map conditions on it directly: `Source`
/// reads an `AtomicBool`, which is what lets Fn+V be described once and still
/// do nothing on a model without the hardware.
pub static VAPOUR_CHAMBER: AtomicBool = AtomicBool::new(false);

/// Records what the identified model supports. Called once the descriptor is
/// known, and again after a resume re-detects the device.
pub fn record(info: &Descriptor) {
    VAPOUR_CHAMBER.store(
        info.has_feature::<feature::VapourChamber>(),
        Ordering::SeqCst,
    );
}

/// Whether the vapour chamber light can be driven at all.
pub fn has_vapour_chamber() -> bool {
    VAPOUR_CHAMBER.load(Ordering::SeqCst)
}

/// Runs `body` with the vapour chamber flag forced on or off, restoring it
/// afterwards.
///
/// The flags here are process-wide and cargo runs a binary's tests on several
/// threads, so every test that pins one takes the same lock first — otherwise
/// one test's model leaks into another's assertions and which test fails is
/// down to timing. A test that panics while holding it poisons the mutex,
/// which is recovered rather than propagated.
#[cfg(test)]
pub fn with_vapour_chamber<T>(present: bool, body: impl FnOnce() -> T) -> T {
    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    let _guard = TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let restore = has_vapour_chamber();
    VAPOUR_CHAMBER.store(present, Ordering::SeqCst);
    let outcome = body();
    VAPOUR_CHAMBER.store(restore, Ordering::SeqCst);
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(features: &'static [&'static str]) -> Descriptor {
        Descriptor {
            model_number_prefix: "RZ09-0000",
            name: "Test Blade",
            pid: 0x0000,
            features,
        }
    }

    /// Detection can fall back to a descriptor with no features, and a resume
    /// re-runs it, so a model without the hardware has to clear the flag rather
    /// than only ever set it.
    #[test]
    fn recording_a_model_without_the_hardware_clears_the_flag() {
        with_vapour_chamber(false, || {
            record(&descriptor(&["fan", "vapour-chamber"]));
            assert!(has_vapour_chamber());

            record(&descriptor(&["fan"]));
            assert!(!has_vapour_chamber());
        });
    }

    /// The Blade 18's own descriptor has to pass the same check the app gates
    /// on, or the feature would be off on the one model that has it.
    #[test]
    fn the_blade_18_descriptor_turns_the_flag_on() {
        with_vapour_chamber(false, || {
            let blade_18 = librazer::descriptor::SUPPORTED
                .iter()
                .find(|device| device.model_number_prefix == "RZ09-05299")
                .expect("the Blade 18 must stay in the supported table");

            record(blade_18);

            assert!(has_vapour_chamber());
        });
    }

    /// An unrecognised Blade is opened through a descriptor built on the fly
    /// with no features at all. Nothing gated may be driven on one, since the
    /// hardware behind the gate is exactly what could not be identified.
    #[test]
    fn an_unrecognised_model_claims_nothing() {
        with_vapour_chamber(true, || {
            record(&descriptor(&[]));

            assert!(!has_vapour_chamber());
        });
    }

    /// Every other supported model has to stay off it: the check is a
    /// whole-string match, so a feature list that merely looks similar must
    /// not pass.
    #[test]
    fn no_other_supported_model_turns_the_flag_on() {
        with_vapour_chamber(false, || {
            for device in librazer::descriptor::SUPPORTED
                .iter()
                .filter(|device| device.model_number_prefix != "RZ09-05299")
            {
                record(device);

                assert!(
                    !has_vapour_chamber(),
                    "{} must not claim a vapour chamber",
                    device.name
                );
            }
        });
    }

    /// The flag is read from threads that never touch the device — the
    /// keyboard hook among them — so what a reader sees has to be the last
    /// value recorded, not whatever it was at startup.
    #[test]
    fn a_later_recording_wins_over_an_earlier_one() {
        with_vapour_chamber(false, || {
            record(&descriptor(&["vapour-chamber"]));
            record(&descriptor(&[]));
            record(&descriptor(&["vapour-chamber"]));

            assert!(has_vapour_chamber());
        });
    }
}
