struct WindowStandbySubscription {
    handle: HPOWERNOTIFY,
}

impl WindowStandbySubscription {
    fn register(hwnd: HWND) -> Option<Self> {
        let handle = unsafe {
            RegisterSuspendResumeNotification(HANDLE(hwnd.0 as *mut _), DEVICE_NOTIFY_WINDOW_HANDLE)
        }
        .ok()?;

        Some(Self { handle })
    }
}

impl Drop for WindowStandbySubscription {
    fn drop(&mut self) {
        if let Err(error) = unsafe { UnregisterSuspendResumeNotification(self.handle) } {
            warn!(?error, "Failed to unregister standby window notification");
        }
    }
}

struct SessionStandbySubscription {
    hwnd: HWND,
}

impl SessionStandbySubscription {
    fn register(hwnd: HWND) -> Option<Self> {
        unsafe { WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) }
            .ok()
            .map(|_| Self { hwnd })
    }
}

impl Drop for SessionStandbySubscription {
    fn drop(&mut self) {
        if let Err(error) = unsafe { WTSUnRegisterSessionNotification(self.hwnd) } {
            warn!(?error, "Failed to unregister standby session notification");
        }
    }
}

