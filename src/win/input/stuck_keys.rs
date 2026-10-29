//! Notices when the keyboard firmware believes a special key is held that is
//! not.
//!
//! A special-key report is `0x04` followed by the code of every special key the
//! firmware counts as down, zero-padded. The reader acts on the first of them,
//! which is the key that changed as long as only one key is down at a time. A
//! firmware that has lost track of a release — seen on a Blade 18 after a
//! failed wake — keeps that key at the front of every report from then on. So
//! every press and release reads as the stuck key, its action runs twice per
//! tap, a capture always records it, and Fn behind it is never seen.
//!
//! Only a restart of the laptop clears the firmware's state, so this does not
//! try to work around it: it recognises the condition and tells the user.

use std::sync::atomic::{AtomicBool, Ordering};

use tracing::{info, warn};

use crate::ui::app::app;
use crate::ui::app_events::OsdEvent;

/// How many other keys must go down while a key stays listed before it is
/// taken to be stuck. Holding one special key through three presses of others
/// is not something a hand does; a stuck key does it within a few taps.
const STUCK_AFTER_PRESSES: u8 = 3;

/// Fn is held through presses of other keys as a matter of course — that is
/// what it is for — so it is never judged stuck.
const FN_KEY_CODE: u8 = 0x0a;

static STUCK: AtomicBool = AtomicBool::new(false);

/// Whether a special key is currently stuck down in the firmware.
pub fn is_stuck() -> bool {
    STUCK.load(Ordering::SeqCst)
}

/// What a report changed about the stuck state, if anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// This key has outlasted enough presses of others to be called stuck.
    Stuck(u8),
    /// The stuck key has finally been reported released.
    Cleared,
}

/// Follows one HID interface's reports.
#[derive(Default)]
pub struct StuckKeyDetector {
    /// Each key the last report listed, with how many presses of other keys it
    /// has stayed down through.
    held: Vec<(u8, u8)>,
    stuck: Option<u8>,
}

impl StuckKeyDetector {
    /// Feeds one special-key report, report id included, and acts on what it
    /// changed: the warning when a key is first found stuck, and its removal
    /// once the key is released after all.
    pub fn observe(&mut self, report: &[u8]) {
        match self.update(report) {
            Some(Change::Stuck(key_code)) => {
                warn!(
                    keycode = key_code,
                    "The keyboard reports a Razer special key as held that is not; \
                     restart the laptop to clear it"
                );
                set_stuck(true);
                app(OsdEvent::CustomAction("Restart to fix Razer keys".to_owned()).into());
            }
            Some(Change::Cleared) => {
                info!("The stuck Razer special key was released");
                set_stuck(false);
            }
            None => {}
        }
    }

    fn update(&mut self, report: &[u8]) -> Option<Change> {
        let [0x04, codes @ ..] = report else {
            return None;
        };
        let now: Vec<u8> = codes.iter().copied().filter(|&code| code != 0).collect();

        let pressed = now
            .iter()
            .filter(|code| !self.held.iter().any(|(held, _)| held == *code))
            .count();
        let pressed = u8::try_from(pressed).unwrap_or(u8::MAX);

        self.held.retain(|(code, _)| now.contains(code));
        for (_, outlasted) in &mut self.held {
            *outlasted = outlasted.saturating_add(pressed);
        }
        for &code in &now {
            if !self.held.iter().any(|(held, _)| *held == code) {
                self.held.push((code, 0));
            }
        }

        if let Some(stuck) = self.stuck {
            if now.contains(&stuck) {
                return None;
            }
            self.stuck = None;
            return Some(Change::Cleared);
        }

        // Only the key at the front matters: it is the one the reader takes
        // every report to be about. A lingering code behind it misleads nothing.
        let front = *now.first()?;
        let (code, _) = self.held.iter().find(|(code, outlasted)| {
            *code == front && *code != FN_KEY_CODE && *outlasted >= STUCK_AFTER_PRESSES
        })?;
        self.stuck = Some(*code);
        Some(Change::Stuck(*code))
    }
}

/// Forgets the stuck state along with the listeners. A detector made for the
/// next listeners starts with nothing held, so it could never see the old key's
/// release — a key freed by the sleep that restarted them would leave the
/// notice up for good. A key still stuck is found again within a few presses.
pub fn reset() {
    set_stuck(false);
}

fn set_stuck(stuck: bool) {
    if STUCK.swap(stuck, Ordering::SeqCst) != stuck {
        crate::ui::webui::notify_settings_changed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(codes: &[u8]) -> Vec<u8> {
        let mut report = vec![0x04];
        report.extend_from_slice(codes);
        report.resize(16, 0);
        report
    }

    fn feed(detector: &mut StuckKeyDetector, reports: &[&[u8]]) -> Vec<Change> {
        reports
            .iter()
            .filter_map(|codes| detector.update(&report(codes)))
            .collect()
    }

    /// The reports captured from the Blade 18 that prompted this: 0xd7 and
    /// 0x27 listed throughout while mic, performance and Fn came and went. The
    /// first report already lists them, so it does not count against them.
    #[test]
    fn a_key_listed_through_other_presses_is_stuck() {
        let mut detector = StuckKeyDetector::default();
        let changes = feed(
            &mut detector,
            &[
                &[0xd7, 0x27, 0xd4],
                &[0xd7, 0x27],
                &[0xd7, 0x27, 0xd3],
                &[0xd7, 0x27],
                &[0xd7, 0x27, 0x0a],
                &[0xd7, 0x27],
            ],
        );
        assert!(changes.is_empty(), "two presses could still be a hand");

        let changes = feed(&mut detector, &[&[0xd7, 0x27, 0xd4], &[0xd7, 0x27]]);
        assert_eq!(changes, vec![Change::Stuck(0xd7)]);
    }

    #[test]
    fn ordinary_taps_are_never_stuck() {
        let mut detector = StuckKeyDetector::default();
        let taps: Vec<&[u8]> = [0xd3u8, 0xd4, 0xd5, 0xd3, 0xd4]
            .iter()
            .flat_map(|code| [std::slice::from_ref(code), &[][..]])
            .collect();
        assert!(feed(&mut detector, &taps).is_empty());
    }

    #[test]
    fn fn_held_through_other_keys_is_not_stuck() {
        let mut detector = StuckKeyDetector::default();
        let changes = feed(
            &mut detector,
            &[
                &[0x0a],
                &[0x0a, 0xd6],
                &[0x0a],
                &[0x0a, 0xd9],
                &[0x0a],
                &[0x0a, 0xd8],
                &[0x0a],
            ],
        );
        assert!(changes.is_empty());
    }

    #[test]
    fn holding_a_key_through_two_others_is_still_a_hand() {
        let mut detector = StuckKeyDetector::default();
        let changes = feed(
            &mut detector,
            &[&[0xd7], &[0xd7, 0xd3], &[0xd7], &[0xd7, 0xd4], &[0xd7], &[]],
        );
        assert!(changes.is_empty());
    }

    #[test]
    fn the_stuck_key_is_reported_once_and_cleared_when_released() {
        let mut detector = StuckKeyDetector::default();
        let changes = feed(
            &mut detector,
            &[
                &[0xd7, 0xd3],
                &[0xd7],
                &[0xd7, 0xd4],
                &[0xd7],
                &[0xd7, 0xd5],
                &[0xd7],
                &[0xd7, 0xd3],
                &[],
            ],
        );
        assert_eq!(changes, vec![Change::Stuck(0xd7), Change::Cleared]);
    }

    /// A code that lingers behind the key at the front never stands in for a
    /// press, so it is not worth telling the user to restart over.
    #[test]
    fn a_lingering_key_behind_the_front_is_not_stuck() {
        let mut detector = StuckKeyDetector::default();
        let changes = feed(
            &mut detector,
            &[
                &[0xd7],
                &[0xd7, 0x27],
                &[0x27],
                &[0x27, 0xd3],
                &[0x27],
                &[0x27, 0xd4],
                &[0x27],
            ],
        );
        assert_eq!(changes, vec![], "0x27 has outlasted only two presses");

        let changes = feed(&mut detector, &[&[0x27, 0xd5]]);
        assert_eq!(
            changes,
            vec![Change::Stuck(0x27)],
            "once at the front, it is what every press reads as"
        );
    }

    #[test]
    fn a_long_held_key_behind_fn_is_not_stuck() {
        let mut detector = StuckKeyDetector::default();
        let changes = feed(
            &mut detector,
            &[
                &[0x0a, 0xd7],
                &[0x0a, 0xd7, 0xd3],
                &[0x0a, 0xd7],
                &[0x0a, 0xd7, 0xd4],
                &[0x0a, 0xd7],
                &[0x0a, 0xd7, 0xd5],
                &[0x0a, 0xd7],
            ],
        );
        assert!(changes.is_empty());
    }

    #[test]
    fn repeated_reports_of_the_same_keys_count_for_nothing() {
        let mut detector = StuckKeyDetector::default();
        let same: Vec<&[u8]> = vec![&[0xd7, 0xd3]; 20];
        assert!(feed(&mut detector, &same).is_empty());
    }

    #[test]
    fn other_reports_are_ignored() {
        let mut detector = StuckKeyDetector::default();
        assert_eq!(detector.update(&[0x02, 0xd7, 0xd3]), None);
        assert_eq!(detector.update(&[]), None);
    }
}
