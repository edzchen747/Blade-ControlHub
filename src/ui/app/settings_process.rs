enum ChildState {
    Running,
    Exited(ExitStatus),
    Unknown,
}

fn child_state(child: &mut Child) -> ChildState {
    match child.try_wait() {
        Ok(None) => ChildState::Running,
        Ok(Some(status)) => ChildState::Exited(status),
        Err(error) => {
            warn!(%error, "Failed to query settings process state");
            ChildState::Unknown
        }
    }
}

fn settings_executable_path() -> std::io::Result<PathBuf> {
    std::env::current_exe()
}

#[cfg(test)]
fn settings_executable_path_from(current_exe: PathBuf) -> PathBuf {
    current_exe
}

fn settings_process_args() -> Vec<&'static str> {
    let mut args = vec![SETTINGS_MODE_ARG];
    if crate::runtime::debug_mode::is_enabled() && !cfg!(debug_assertions) {
        args.push(crate::runtime::debug_mode::DEBUG_MODE_ARG);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn settings_executable_path_is_current_binary() {
        let path = settings_executable_path_from(PathBuf::from(
            r"C:\Tools\Blade\target\debug\blade-controlhub.exe",
        ));

        assert_eq!(
            path,
            Path::new(r"C:\Tools\Blade\target\debug")
                .join(format!("blade-controlhub{}", std::env::consts::EXE_SUFFIX))
        );
    }

    #[test]
    fn settings_process_args_select_settings_mode() {
        assert_eq!(settings_process_args(), vec![SETTINGS_MODE_ARG]);
    }

    #[test]
    fn osd_disable_guard_count_suppresses_osd_without_changing_user_preference() {
        let mut core = AppCore::new();
        assert!(core.should_show_osd());

        core.osd_disable_guards = 1;
        assert!(!core.should_show_osd());

        core.osd_disable_guards = 0;
        assert!(core.should_show_osd());
    }

    #[test]
    fn settings_window_suppresses_osd_only_when_open_and_focused() {
        assert!(!settings_window_should_suppress_osd(false, false));
        assert!(!settings_window_should_suppress_osd(false, true));
        assert!(!settings_window_should_suppress_osd(true, false));
        assert!(settings_window_should_suppress_osd(true, true));
    }
}
