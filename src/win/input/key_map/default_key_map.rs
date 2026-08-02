pub static KEY_MAP: Lazy<HashMap<KeyType, KeyEventAction>> = Lazy::new(|| {
    HashMap::from([
        (
            vkey::Key::B.into(),
            KeyEventAction::new(
                Box::new(|| device().cycle_battery_limit()),
                vec![Source::IsTrue(&FN_PRESSED)],
            ),
        ),
        (
            vkey::Key::P.into(),
            KeyEventAction::new(
                Box::new(|| {
                    device().cycle_perf_mode();
                }),
                vec![Source::IsTrue(&FN_PRESSED)],
            ),
        ),
        (
            vkey::Key::R.into(),
            KeyEventAction::new(
                Box::new(|| {
                    device().cycle_refresh_rate();
                }),
                vec![Source::IsTrue(&FN_PRESSED)],
            ),
        ),
        (
            vkey::Key::T.into(),
            KeyEventAction::new(
                Box::new(|| {
                    toggle_trackpad();
                }),
                vec![Source::IsTrue(&FN_PRESSED)],
            ),
        ),
        (
            vkey::Key::V.into(),
            KeyEventAction::new(
                Box::new(|| {
                    device().toggle_vc();
                }),
                vec![Source::IsTrue(&FN_PRESSED)],
            ),
        ),
        (
            vkey::Key::F1.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(173)]).trigger();
                }),
                vec![
                    Source::IsXOR(&DEFAULT_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            vkey::Key::F2.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(174)]).trigger();
                }),
                vec![
                    Source::IsXOR(&DEFAULT_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            vkey::Key::F3.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(175)]).trigger();
                }),
                vec![
                    Source::IsXOR(&DEFAULT_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            vkey::Key::F4.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::MetaLeft, Key::KeyP]).trigger();
                }),
                vec![
                    Source::IsXOR(&DEFAULT_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            vkey::Key::F5.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(177)]).trigger();
                }),
                vec![
                    Source::IsXOR(&DEFAULT_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            vkey::Key::F6.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(179)]).trigger();
                }),
                vec![
                    Source::IsXOR(&DEFAULT_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            vkey::Key::F7.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(176)]).trigger();
                }),
                vec![
                    Source::IsXOR(&DEFAULT_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            vkey::Key::F8.into(),
            KeyEventAction::new(
                Box::new(|| {
                    device().adjust_screen_brightness(-10);
                }),
                vec![
                    Source::IsXOR(&DEFAULT_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            vkey::Key::F9.into(),
            KeyEventAction::new(
                Box::new(|| {
                    device().adjust_screen_brightness(10);
                }),
                vec![
                    Source::IsXOR(&DEFAULT_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            vkey::Key::F10.into(),
            KeyEventAction::new(
                Box::new(|| {
                    device().keyboard_light_down();
                }),
                vec![
                    Source::IsXOR(&DEFAULT_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            vkey::Key::F11.into(),
            KeyEventAction::new(
                Box::new(|| {
                    device().keyboard_light_up();
                }),
                vec![
                    Source::IsXOR(&DEFAULT_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            vkey::Key::F12.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::PrintScreen]).trigger();
                }),
                vec![
                    Source::IsXOR(&DEFAULT_MULTIMEDIA_KEYS, &FN_PRESSED),
                    Source::IsFalse(&ALT_PRESSED),
                ],
            ),
        ),
        (
            razer_key::Key::Mic.into(),
            KeyEventAction::new(
                Box::new(|| {
                    audio::toggle_audio_mute(AudioType::Mic);
                }),
                vec![],
            ),
        ),
        (
            razer_key::Key::Trackpad.into(),
            KeyEventAction::new(
                Box::new(|| {
                    toggle_trackpad();
                }),
                vec![],
            ),
        ),
        (
            razer_key::Key::Perf.into(),
            KeyEventAction::new(
                Box::new(|| {
                    device().cycle_perf_mode();
                }),
                vec![],
            ),
        ),
        (
            razer_key::Key::CoPilot.into(),
            KeyEventAction::new(
                Box::new(|| {
                    device().cycle_rgb_mode();
                }),
                vec![],
            ),
        ),
        (
            razer_key::Key::Home.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(36)]).trigger();
                }),
                vec![],
            ),
        ),
        (
            razer_key::Key::Up.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(38)]).trigger();
                }),
                vec![],
            ),
        ),
        (
            razer_key::Key::PgUp.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(33)]).trigger();
                }),
                vec![],
            ),
        ),
        (
            razer_key::Key::Left.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(37)]).trigger();
                }),
                vec![],
            ),
        ),
        (
            razer_key::Key::Right.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(39)]).trigger();
                }),
                vec![],
            ),
        ),
        (
            razer_key::Key::End.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(35)]).trigger();
                }),
                vec![],
            ),
        ),
        (
            razer_key::Key::Down.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(40)]).trigger();
                }),
                vec![],
            ),
        ),
        (
            razer_key::Key::PgDn.into(),
            KeyEventAction::new(
                Box::new(|| {
                    KeyCombo::new(&[Key::Unknown(34)]).trigger();
                }),
                vec![],
            ),
        ),
    ])
});
