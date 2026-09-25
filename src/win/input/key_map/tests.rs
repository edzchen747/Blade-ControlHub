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
