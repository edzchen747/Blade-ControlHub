macro_rules! define_keymap {
    ($($name:ident = $val:expr),*) => {
        #[derive(Debug, PartialEq, Copy, Clone, Eq, Hash)]
        pub enum Key {
            $($name,)*
            Unknown(u8),
        }

        impl From<u8> for Key {
            fn from(code: u8) -> Self {
                match code {
                    $($val => Key::$name,)*
                    _ => Key::Unknown(code),
                }
            }
        }
    }
}

define_keymap! {
    // Letters with Fn functions
    B = 0x05,
    P = 0x13,
    R = 0x15,
    T = 0x17,
    V = 0x19,

    // Function Keys
    F1 = 0x3a,
    F2 = 0x3b,
    F3 = 0x3c,
    F4 = 0x3d,
    F5 = 0x3e,
    F6 = 0x3f,
    F7 = 0x40,
    F8 = 0x41,
    F9 = 0x42,
    F10 = 0x43,
    F11 = 0x44,
    F12 = 0x45,

    // Dedicated Keys
    Mic      = 0xd4,
    Trackpad = 0xdd,
    Perf     = 0xd3,
    M1       = 0x24,
    M2       = 0x25,
    M3       = 0x26,
    M4       = 0x27,
    Game     = 0x03,
    CoPilot  = 0xd2,
    Home     = 0xd5,
    Up       = 0xd6,
    PgUp     = 0xd7,
    Left     = 0xd8,
    Right    = 0xd9,
    End      = 0xda,
    Down     = 0xdb,
    PgDn     = 0xdc
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
            _ => Self::Unknown(0),
        }
    }
}
