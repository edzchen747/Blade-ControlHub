struct PowerSettingStandbySubscription {
    registrations: Vec<PowerSettingRegistration>,
}

impl PowerSettingStandbySubscription {
    fn register(hwnd: HWND) -> Option<Self> {
        let registrations = [
            ("console_display_state", GUID_CONSOLE_DISPLAY_STATE),
            ("session_display_status", GUID_SESSION_DISPLAY_STATUS),
            ("monitor_power_on", GUID_MONITOR_POWER_ON),
        ]
        .into_iter()
        .filter_map(|(name, guid)| PowerSettingRegistration::register(hwnd, name, &guid))
        .collect::<Vec<_>>();

        (!registrations.is_empty()).then_some(Self { registrations })
    }
}

impl Drop for PowerSettingStandbySubscription {
    fn drop(&mut self) {
        self.registrations.clear();
    }
}

struct PowerSettingRegistration {
    name: &'static str,
    handle: HPOWERNOTIFY,
}

impl PowerSettingRegistration {
    fn register(hwnd: HWND, name: &'static str, guid: &windows::core::GUID) -> Option<Self> {
        let handle = unsafe {
            RegisterPowerSettingNotification(
                HANDLE(hwnd.0 as *mut _),
                guid,
                DEVICE_NOTIFY_WINDOW_HANDLE,
            )
        };

        match handle {
            Ok(handle) => {
                info!(
                    setting = name,
                    "Registered standby power setting notification"
                );
                Some(Self { name, handle })
            }
            Err(error) => {
                warn!(
                    ?error,
                    setting = name,
                    "Standby power setting registration failed"
                );
                None
            }
        }
    }
}

impl Drop for PowerSettingRegistration {
    fn drop(&mut self) {
        if let Err(error) = unsafe { UnregisterPowerSettingNotification(self.handle) } {
            warn!(
                ?error,
                setting = self.name,
                "Failed to unregister standby power setting notification"
            );
        }
    }
}

