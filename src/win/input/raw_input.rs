use windows::{
    Win32::Foundation::*, Win32::System::LibraryLoader::GetModuleHandleW, Win32::UI::Input::*,
    Win32::UI::WindowsAndMessaging::*, core::*,
};

pub struct RawInputListener {
    _hwnd: HWND,
}

impl RawInputListener {
    pub fn new() -> Result<Self> {
        unsafe {
            let instance = GetModuleHandleW(None)?;
            let window_class = w!("RawInputClass");

            let wc = WNDCLASSW {
                lpfnWndProc: Some(Self::window_proc),
                hInstance: instance.into(),
                lpszClassName: window_class,
                ..Default::default()
            };

            RegisterClassW(&wc);

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                window_class,
                w!("RawInputWindow"),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                HMENU::default(),
                instance,
                None,
            )?; // Extracted via ?

            let rid = RAWINPUTDEVICE {
                usUsagePage: 0x01,
                usUsage: 0x06,
                dwFlags: RIDEV_INPUTSINK,
                hwndTarget: hwnd,
            };

            RegisterRawInputDevices(&[rid], std::mem::size_of::<RAWINPUTDEVICE>() as u32)?;

            Ok(Self { _hwnd: hwnd })
        }
    }

    pub fn start() {
        std::thread::spawn(move || {
            println!("started raw input");
            Self::new()
                .expect("Fatal internal error: raw input listener")
                .run();
        });
    }

    fn run(&self) {
        unsafe {
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
                DispatchMessageW(&msg);
            }
        }
    }

    unsafe fn get_device_ids(h_device: HANDLE) -> (String, String) {
        let mut size: u32 = 0;
        // 2. Wrapped in unsafe block for Rust 2024 compliance
        unsafe {
            let _ = GetRawInputDeviceInfoW(h_device, RIDI_DEVICENAME, None, &mut size);

            let mut buffer = vec![0u16; size as usize];
            if GetRawInputDeviceInfoW(
                h_device,
                RIDI_DEVICENAME,
                Some(buffer.as_mut_ptr() as _),
                &mut size,
            ) != u32::MAX
            {
                let path = String::from_utf16_lossy(&buffer);

                let vid = path
                    .split("VID_")
                    .nth(1)
                    .and_then(|s| s.get(0..4))
                    .unwrap_or("????")
                    .to_string();

                let pid = path
                    .split("PID_")
                    .nth(1)
                    .and_then(|s| s.get(0..4))
                    .unwrap_or("????")
                    .to_string();

                return (vid, pid);
            }
        }
        ("????".to_string(), "????".to_string())
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_INPUT {
            let mut size: u32 = 0;
            // 3. HRAWINPUT construction changed to require a pointer
            let h_raw_input = HRAWINPUT(lparam.0 as *mut core::ffi::c_void);

            unsafe {
                GetRawInputData(
                    h_raw_input,
                    RID_INPUT,
                    None,
                    &mut size,
                    std::mem::size_of::<RAWINPUTHEADER>() as u32,
                );

                let mut buffer = vec![0u8; size as usize];
                if GetRawInputData(
                    h_raw_input,
                    RID_INPUT,
                    Some(buffer.as_mut_ptr() as _),
                    &mut size,
                    std::mem::size_of::<RAWINPUTHEADER>() as u32,
                ) != u32::MAX
                {
                    let raw = &*(buffer.as_ptr() as *const RAWINPUT);
                    if raw.header.dwType == RIM_TYPEKEYBOARD.0 {
                        let (vid, pid) = Self::get_device_ids(raw.header.hDevice);
                        let kb = raw.data.keyboard;

                        let state = if (kb.Flags as u32 & RI_KEY_BREAK) != 0 {
                            "UP  "
                        } else {
                            "DOWN"
                        };

                        println!(
                            "[VID: 0x{} PID: 0x{}] | {} | VK: 0x{:02X}",
                            vid, pid, state, kb.VKey
                        );
                    }
                }
            }
        }
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }
}
