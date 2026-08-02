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

    #[test]
    fn key_combo_ignores_keys_after_four_slots() {
        let combo = KeyCombo::new(&[
            Key::ControlLeft,
            Key::ShiftLeft,
            Key::Alt,
            Key::KeyP,
            Key::KeyA,
        ]);

        assert_eq!(combo.into_iter().count(), 4);
        assert!(!combo.events().contains(&EventType::KeyPress(Key::KeyA)));
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
