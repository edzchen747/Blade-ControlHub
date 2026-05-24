macro_rules! define_keymap {
    ($($name:ident = $val:expr),*) => {
        #[derive(Debug, PartialEq, Copy, Clone, Eq, Hash)]
        pub enum Key {
            $($name,)*
            Unknown,
        }

        impl From<u8> for Key {
            fn from(code: u8) -> Self {
                match code {
                    $($val => Key::$name,)*
                    _ => Key::Unknown,
                }
            }
        }
    }
}

define_keymap! {
    // Letters with Fn functions
    B = 0x42,
    P = 0x50,
    R = 0x52,
    T = 0x54,
    V = 0x56,

    // Function Keys
    F1 = 0x70,
    F2 = 0x71,
    F3 = 0x72,
    F4 = 0x73,
    F5 = 0x74,
    F6 = 0x75,
    F7 = 0x76,
    F8 = 0x77,
    F9 = 0x78,
    F10 = 0x79,
    F11 = 0x7a,
    F12 = 0x7b
}

impl From<rdev::Key> for Key {
    fn from(key: rdev::Key) -> Self {
        match key {
            rdev::Key::KeyB => Self::B,
            rdev::Key::KeyP => Self::P,
            rdev::Key::KeyR => Self::R,
            rdev::Key::KeyT => Self::T,
            rdev::Key::KeyV => Self::V,
            rdev::Key::F1 => Self::F1,
            rdev::Key::F2 => Self::F2,
            rdev::Key::F3 => Self::F3,
            rdev::Key::F4 => Self::F4,
            rdev::Key::F5 => Self::F5,
            rdev::Key::F6 => Self::F6,
            rdev::Key::F7 => Self::F7,
            rdev::Key::F8 => Self::F8,
            rdev::Key::F9 => Self::F9,
            rdev::Key::F10 => Self::F10,
            rdev::Key::F11 => Self::F11,
            rdev::Key::F12 => Self::F12,
            _ => Self::Unknown,
        }
    }
}
