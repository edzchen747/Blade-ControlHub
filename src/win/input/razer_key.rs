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
