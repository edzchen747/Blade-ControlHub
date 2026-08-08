impl AppCore {
    fn new() -> Self {
        Self {
            running: true,
            osd_enabled: true,
            osd_disable_guards: 0,
            settings_osd_disable_guard: None,
            settings: SettingsStore::new(),
            pending_side_effects: Vec::new(),
        }
    }

    fn should_show_osd(&self) -> bool {
        self.osd_enabled && self.osd_disable_guards == 0
    }
}

pub fn app(event: AppEvent) {
    let Some(ctx) = APP_CONTEXT.get_or_timeout() else {
        warn!("Dropping app event before app context initialization completed");
        return;
    };

    if let Some(side) = EventDispatcher::dispatch(event) {
        core(&ctx).pending_side_effects.push(side);
    }

    let pending = {
        let mut core = core(&ctx);
        if !core.running {
            return;
        }
        std::mem::take(&mut core.pending_side_effects)
    };

    for side in pending {
        match side {
            SideEffect::ToggleSettings | SideEffect::OpenSettings => {
                open_or_toggle_settings(&ctx);
            }
            SideEffect::Shutdown => {
                shutdown_runtime(&ctx, "application shutdown");
                return;
            }
            SideEffect::Restart(code) => {
                if let Err(error) = spawn_replacement_app(code) {
                    warn!(%error, "Failed to spawn replacement app for restart");
                    return;
                }
                shutdown_runtime(&ctx, "application restart");
                return;
            }
            SideEffect::EnableOsd(enable) => {
                core(&ctx).osd_enabled = enable;
            }
            SideEffect::PerfMode(mode) => {
                TrayManager::set_tray_icon(mode);
            }
        }
    }

    let osd_params = match event {
        AppEvent::OsdEvent(osd_event) => osd_event.as_params(),
        _ => None,
    };
    if let Some(osd_params) = osd_params {
        let core = core(&ctx);
        if core.should_show_osd() {
            OsdController::show(osd_params);
        }
    }
}

pub fn set_osd_enabled(enabled: bool) {
    app(OsdEvent::EnableOSD(enabled).into());
}

fn acquire_osd_disable_guard() {
    let Some(ctx) = APP_CONTEXT.get() else {
        warn!("Ignoring OSD disable before app initialization");
        return;
    };
    let mut core = core(ctx);
    core.osd_disable_guards = core.osd_disable_guards.saturating_add(1);
}

fn release_osd_disable_guard() {
    let Some(ctx) = APP_CONTEXT.get() else {
        return;
    };
    let mut core = core(ctx);
    if core.osd_disable_guards == 0 {
        warn!("Attempted to release an OSD disable guard that was not held");
        return;
    }
    core.osd_disable_guards -= 1;
}

pub fn set_settings_window_state(open: bool, focused: bool) {
    let Some(ctx) = APP_CONTEXT.get() else {
        warn!(
            open,
            focused, "Ignoring settings-window visibility before app initialization"
        );
        return;
    };

    if settings_window_should_suppress_osd(open, focused) {
        if core(ctx).settings_osd_disable_guard.is_none() {
            let guard = OsdDisableGuard::new();
            core(ctx).settings_osd_disable_guard = Some(guard);
        }
    } else {
        let guard = core(ctx).settings_osd_disable_guard.take();
        drop(guard);
    }
}

const fn settings_window_should_suppress_osd(open: bool, focused: bool) -> bool {
    open && focused
}

pub struct OsdDisableGuard;

impl OsdDisableGuard {
    pub fn new() -> Self {
        acquire_osd_disable_guard();
        Self
    }
}

impl Default for OsdDisableGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for OsdDisableGuard {
    fn drop(&mut self) {
        release_osd_disable_guard();
    }
}

