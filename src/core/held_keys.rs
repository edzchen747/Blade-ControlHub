//! Which on-screen display a held key is currently driving.
//!
//! The keyboard hook knows a *press* from a *repeat*; the overlay, several
//! threads away, only sees "brightness changed". This is the one place the two
//! meet. A key that maps to an overlay registers here when it goes down, and the
//! overlay asks whether a given update belongs to a press it has not shown yet
//! or to a key that is simply still held.
//!
//! That is what lets two held keys keep two stacked overlays live: a fresh
//! press brings its card to the front, a repeat updates its card where it sits.
//! Kinds are `OsdIcon::kind_key` values.

use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

const KINDS: usize = 16;

static HELD: [AtomicU8; KINDS] = [const { AtomicU8::new(0) }; KINDS];
static PRESSES: [AtomicU32; KINDS] = [const { AtomicU32::new(0) }; KINDS];

/// A key driving `kind` went down for the first time.
pub fn press(kind: u8) {
    let Some(index) = slot(kind) else { return };
    PRESSES[index].fetch_add(1, Ordering::SeqCst);
    HELD[index].fetch_add(1, Ordering::SeqCst);
}

/// A key driving `kind` came up, or stopped being held for any other reason.
pub fn release(kind: u8) {
    let Some(index) = slot(kind) else { return };
    let _ = HELD[index].fetch_update(Ordering::SeqCst, Ordering::SeqCst, |held| {
        held.checked_sub(1)
    });
}

/// How many presses of `kind` there have been. An overlay card keeps the value
/// it last saw, and any difference means a new press since.
pub fn press_count(kind: u8) -> u32 {
    slot(kind).map_or(0, |index| PRESSES[index].load(Ordering::SeqCst))
}

/// Whether an update for `kind` is only a held key repeating: a key is down and
/// no new press has happened since the card last looked.
pub fn is_repeat(kind: u8, seen_presses: u32) -> bool {
    let Some(index) = slot(kind) else {
        return false;
    };
    HELD[index].load(Ordering::SeqCst) > 0 && PRESSES[index].load(Ordering::SeqCst) == seen_presses
}

/// Whether any key driving `kind` is down right now.
pub fn is_held(kind: u8) -> bool {
    slot(kind).is_some_and(|index| HELD[index].load(Ordering::SeqCst) > 0)
}

fn slot(kind: u8) -> Option<usize> {
    let index = usize::from(kind);
    (index < KINDS).then_some(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Each test uses its own kind: the counters are process-wide and cargo
    // runs the tests of one binary on several threads.

    #[test]
    fn a_new_press_is_not_a_repeat_but_the_held_key_after_it_is() {
        let kind = 12;
        let seen = press_count(kind);

        press(kind);
        assert!(
            !is_repeat(kind, seen),
            "the press itself must be shown fresh"
        );

        let seen = press_count(kind);
        assert!(is_repeat(kind, seen), "the same key still down is a repeat");

        release(kind);
    }

    #[test]
    fn a_kind_is_held_only_while_a_key_for_it_is_down() {
        let kind = 11;
        assert!(!is_held(kind));
        press(kind);
        assert!(is_held(kind));
        release(kind);
        assert!(!is_held(kind));
    }

    #[test]
    fn an_update_with_no_key_down_is_never_a_repeat() {
        let kind = 13;
        press(kind);
        let seen = press_count(kind);
        release(kind);

        assert!(!is_repeat(kind, seen));
    }

    #[test]
    fn a_second_press_while_held_is_fresh_again() {
        let kind = 14;
        press(kind);
        let seen = press_count(kind);
        press(kind);

        assert!(!is_repeat(kind, seen));
        release(kind);
        release(kind);
    }

    #[test]
    fn releasing_more_than_was_pressed_does_not_wrap() {
        let kind = 15;
        release(kind);
        let seen = press_count(kind);

        assert!(!is_repeat(kind, seen));
    }

    #[test]
    fn a_kind_outside_the_table_is_ignored() {
        press(200);
        assert_eq!(press_count(200), 0);
        assert!(!is_repeat(200, 0));
        release(200);
    }
}
