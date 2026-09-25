#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_combo_events_press_in_order_and_release_in_reverse() {
        let combo = KeyCombo::new(&[Key::ControlLeft, Key::ShiftLeft, Key::KeyP]);

        assert_eq!(
            combo.events(),
            vec![
                EventType::KeyPress(Key::ControlLeft),
                EventType::KeyPress(Key::ShiftLeft),
                EventType::KeyPress(Key::KeyP),
                EventType::KeyRelease(Key::KeyP),
                EventType::KeyRelease(Key::ShiftLeft),
                EventType::KeyRelease(Key::ControlLeft),
            ]
        );
    }

    /// A custom binding can hold every modifier at once, so the combo must not
    /// drop keys past a fixed width the way it used to.
    #[test]
    fn key_combo_keeps_every_key_it_is_given() {
        let combo = KeyCombo::new(&[
            Key::ControlLeft,
            Key::Alt,
            Key::ShiftLeft,
            Key::MetaLeft,
            Key::KeyA,
        ]);

        assert_eq!(combo.into_iter().count(), 5);
        assert!(combo.events().contains(&EventType::KeyPress(Key::KeyA)));
        assert_eq!(
            combo.events().last(),
            Some(&EventType::KeyRelease(Key::ControlLeft)),
            "the first key pressed is the last released"
        );
    }

    /// Fn+V drives hardware only the Blade 18 has. It stays in the map either
    /// way so it can be described once, and fails its condition instead — a
    /// refusal the hook reads as "not handled", which is what lets the
    /// keystroke reach Windows on every other model.
    ///
    /// Only the refusing combinations are exercised. Satisfying both
    /// conditions would run the action, and the action drives the light on the
    /// machine the tests are running on.
    #[test]
    fn fn_v_needs_both_fn_and_the_hardware() {
        use crate::core::capabilities::with_vapour_chamber;
        use crate::core::shared_state::FN_PRESSED;

        let entry = KEY_MAP
            .get(&vkey::Key::V)
            .expect("Fn+V must stay described in the built-in map");
        let restore = FN_PRESSED.load(Ordering::SeqCst);

        FN_PRESSED.store(true, Ordering::SeqCst);
        let without_hardware = with_vapour_chamber(false, || entry.execute());

        FN_PRESSED.store(false, Ordering::SeqCst);
        let without_fn = with_vapour_chamber(true, || entry.execute());

        FN_PRESSED.store(restore, Ordering::SeqCst);

        assert!(
            !without_hardware,
            "without the vent the key must fall through to Windows"
        );
        assert!(!without_fn, "V on its own is still just V");
    }

    #[test]
    fn key_event_action_executes_only_when_conditions_match() {
        let enabled = AtomicBool::new(true);
        let disabled = AtomicBool::new(false);
        let action = KeyEventAction::new(Box::new(|| {}), vec![Source::IsTrue(&enabled)]);
        let blocked = KeyEventAction::new(Box::new(|| {}), vec![Source::IsTrue(&disabled)]);

        assert!(action.execute());
        assert!(!blocked.execute());
    }
}
