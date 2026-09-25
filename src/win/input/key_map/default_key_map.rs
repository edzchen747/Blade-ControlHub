/// What the Fn layer does before the user rebinds anything.
///
/// Razer's own special keys are deliberately absent: which code each key sends
/// differs between models, so a table of them here would be wrong on most
/// Blades. They are bound on the Keys page instead, by pressing the key.
pub static KEY_MAP: Lazy<HashMap<vkey::Key, KeyEventAction>> = Lazy::new(|| {
    HashMap::from([
        (
            vkey::Key::B,
            KeyEventAction::new(
                Box::new(|| device().cycle_battery_limit()),
                vec![Source::IsTrue(&FN_PRESSED)],
            ),
        ),
        (
            vkey::Key::P,
            KeyEventAction::new(
                Box::new(|| {
                    device().cycle_perf_mode();
                }),
                vec![Source::IsTrue(&FN_PRESSED)],
            ),
        ),
        (
            vkey::Key::R,
            KeyEventAction::new(
                Box::new(|| {
                    device().cycle_refresh_rate();
                }),
                vec![Source::IsTrue(&FN_PRESSED)],
            ),
        ),
        (
            vkey::Key::T,
            KeyEventAction::new(
                Box::new(|| {
                    toggle_trackpad();
                }),
                vec![Source::IsTrue(&FN_PRESSED)],
            ),
        ),
        (
            vkey::Key::V,
            KeyEventAction::new(
                Box::new(|| {
                    device().toggle_vc();
                }),
                // Only the Blade 18 has the vent to light. Failing the
                // condition rather than dropping the entry means Fn+V still
                // reaches Windows on every other model.
                vec![Source::IsTrue(&FN_PRESSED), Source::IsTrue(&VAPOUR_CHAMBER)],
            ),
        ),
        (
            vkey::Key::F1,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(173)]).trigger();
                }),
                vec![
                    Source::IsXOR(&PRIMARY_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            vkey::Key::F2,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(174)]).trigger();
                }),
                vec![
                    Source::IsXOR(&PRIMARY_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            vkey::Key::F3,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(175)]).trigger();
                }),
                vec![
                    Source::IsXOR(&PRIMARY_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            vkey::Key::F4,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::MetaLeft, Key::KeyP]).trigger();
                }),
                vec![
                    Source::IsXOR(&PRIMARY_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            vkey::Key::F5,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(177)]).trigger();
                }),
                vec![
                    Source::IsXOR(&PRIMARY_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            vkey::Key::F6,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(179)]).trigger();
                }),
                vec![
                    Source::IsXOR(&PRIMARY_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            vkey::Key::F7,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(176)]).trigger();
                }),
                vec![
                    Source::IsXOR(&PRIMARY_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            vkey::Key::F8,
            KeyEventAction::new(
                Box::new(|| {
                    device().adjust_screen_brightness(-10);
                }),
                vec![
                    Source::IsXOR(&PRIMARY_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            vkey::Key::F9,
            KeyEventAction::new(
                Box::new(|| {
                    device().adjust_screen_brightness(10);
                }),
                vec![
                    Source::IsXOR(&PRIMARY_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            vkey::Key::F10,
            KeyEventAction::new(
                Box::new(|| {
                    device().keyboard_light_down();
                }),
                vec![
                    Source::IsXOR(&PRIMARY_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            vkey::Key::F11,
            KeyEventAction::new(
                Box::new(|| {
                    device().keyboard_light_up();
                }),
                vec![
                    Source::IsXOR(&PRIMARY_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            vkey::Key::F12,
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::PrintScreen]).trigger();
                }),
                vec![
                    Source::IsXOR(&PRIMARY_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
    ])
});
