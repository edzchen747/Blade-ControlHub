struct StandbyWindow {
    hwnd: HWND,
}

impl StandbyWindow {
    fn create() -> Option<Self> {
        create_standby_window().map(|hwnd| Self { hwnd })
    }
}

impl Drop for StandbyWindow {
    fn drop(&mut self) {
        if self.hwnd.0.is_null() {
            return;
        }

        if let Err(error) = unsafe { DestroyWindow(self.hwnd) } {
            warn!(?error, "Failed to destroy standby monitor window");
        }
    }
}

fn create_standby_window() -> Option<HWND> {
    unsafe {
        let instance = match GetModuleHandleW(None) {
            Ok(h) => h,
            Err(e) => {
                error!(error = ?e, "GetModuleHandleW failed; standby monitor will not start");
                return None;
            }
        };
        let class_name: Vec<u16> = "RazerPowerListener\0".encode_utf16().collect();

        let wnd_class = WNDCLASSW {
            hInstance: instance.into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            lpfnWndProc: Some(wnd_proc),
            ..Default::default()
        };
        let _ = RegisterClassW(&wnd_class);

        match CreateWindowExW(
            Default::default(),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(class_name.as_ptr()),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            instance,
            None,
        ) {
            Ok(hwnd) => Some(hwnd),
            Err(e) => {
                error!(error = ?e, "CreateWindowExW failed; standby monitor will not start");
                None
            }
        }
    }
}

fn standby_state() -> std::sync::MutexGuard<'static, StandbyState> {
    match STATE_MANAGER.state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!("Standby state mutex poisoned; recovering");
            poisoned.into_inner()
        }
    }
}

fn update_standby_state_from_event(event_type: u32) -> bool {
    let mut lock = standby_state();
    match event_type {
        PBT_APMSUSPEND => {
            *lock = StandbyState::Sleep;
            true
        }
        PBT_APMRESUMEAUTOMATIC => {
            *lock = StandbyState::Wake;
            true
        }
        PBT_APMRESUMESUSPEND => {
            *lock = StandbyState::Wake;
            true
        }
        _ => false,
    }
}

fn update_standby_state_from_watchdog_resume() -> bool {
    let mut lock = standby_state();
    if *lock == StandbyState::Wake {
        return false;
    }

    *lock = StandbyState::Wake;
    true
}

fn update_standby_state_from_session_resume() -> bool {
    let mut lock = standby_state();
    if *lock == StandbyState::Wake {
        return false;
    }

    *lock = StandbyState::Wake;
    true
}

unsafe fn update_standby_state_from_power_setting_lparam(lparam: LPARAM) -> bool {
    let setting = lparam.0 as *const POWERBROADCAST_SETTING;
    if setting.is_null() {
        return false;
    }

    let setting = unsafe { &*setting };
    let Some(value) = power_setting_data_u32(setting) else {
        return false;
    };
    let Some((name, state)) = display_power_setting_state(setting.PowerSetting, value) else {
        return false;
    };

    info!(
        setting = name,
        value,
        state = ?state,
        "Windows display power setting event received"
    );
    let changed = update_standby_state_from_display_power(state);
    state == StandbyState::Sleep && changed
}

fn power_setting_data_u32(setting: &POWERBROADCAST_SETTING) -> Option<u32> {
    if setting.DataLength < std::mem::size_of::<u32>() as u32 {
        return None;
    }

    Some(unsafe { std::ptr::read_unaligned(setting.Data.as_ptr() as *const u32) })
}

fn display_power_setting_state(
    guid: windows::core::GUID,
    value: u32,
) -> Option<(&'static str, StandbyState)> {
    let name = display_power_setting_name(guid)?;
    match value {
        DISPLAY_POWER_OFF => Some((name, StandbyState::Sleep)),
        DISPLAY_POWER_ON => Some((name, StandbyState::Wake)),
        _ => None,
    }
}

fn display_power_setting_name(guid: windows::core::GUID) -> Option<&'static str> {
    if guid == GUID_CONSOLE_DISPLAY_STATE {
        Some("console_display_state")
    } else if guid == GUID_SESSION_DISPLAY_STATUS {
        Some("session_display_status")
    } else if guid == GUID_MONITOR_POWER_ON {
        Some("monitor_power_on")
    } else {
        None
    }
}

fn update_standby_state_from_display_power(state: StandbyState) -> bool {
    let mut lock = standby_state();
    if *lock == state {
        return false;
    }

    *lock = state;
    true
}

fn session_change_is_wake(event_type: u32) -> bool {
    event_type == WTS_SESSION_UNLOCK_EVENT
}

fn resume_gap_detected(elapsed: Duration) -> bool {
    elapsed >= STANDBY_RESUME_GAP_THRESHOLD
}

