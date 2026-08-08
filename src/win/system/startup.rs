use core::time;
use std::env;
use std::fs;
use std::process::Command;
use std::thread;
use tracing::{error, info, warn};

pub struct Startup;

impl Startup {
    const TASK_NAME: &'static str = "Blade ControlHub";

    pub fn register(run_as_admin: bool) {
        if let Err(error) = thread::Builder::new()
            .name("blade-startup-register".to_string())
            .spawn(move || {
                thread::sleep(time::Duration::from_secs(2));
                Self::register_now(run_as_admin);
            })
        {
            warn!(%error, "Failed to start startup registration thread");
        }
    }

    /// Synchronous variant of [`Self::register`], for callers that must leave
    /// the task in the correct state before a process relaunch.
    pub fn register_now(run_as_admin: bool) {
        let exe_path = match env::current_exe() {
            Ok(p) => p,
            Err(e) => {
                error!(error = %e, "Failed to get executable path");
                return;
            }
        };
        let exe_dir = match exe_path.parent() {
            Some(p) => p,
            None => {
                error!("Unable to get executable directory");
                return;
            }
        };

        let path_str = match exe_path.to_str() {
            Some(s) => s,
            None => {
                error!("Executable path contains invalid Unicode");
                return;
            }
        };
        let dir_str = match exe_dir.to_str() {
            Some(s) => s,
            None => {
                error!("Directory path contains invalid Unicode");
                return;
            }
        };

        let mut xml_content = include_str!("../../win/task.xml").to_string();

        xml_content = xml_content.replace("__EXE_PATH__", path_str);
        xml_content = xml_content.replace("__EXE_DIR__", dir_str);
        xml_content = xml_content.replace("__RUN_LEVEL__", Self::run_level(run_as_admin));

        let temp_xml_path = env::temp_dir().join("blade_task_config.xml");
        if let Err(e) = fs::write(&temp_xml_path, xml_content) {
            error!(error = %e, "Failed to write temporary task XML file");
            return;
        }

        let temp_xml_str = match temp_xml_path.to_str() {
            Some(s) => s,
            None => {
                error!("Temporary file path contains invalid Unicode");
                return;
            }
        };

        let status = match Command::new("schtasks")
            .args([
                "/Create",
                "/TN",
                Self::TASK_NAME,
                "/XML",
                temp_xml_str,
                "/F",
            ])
            .status()
        {
            Ok(s) => s,
            Err(e) => {
                error!(error = %e, "Failed to execute schtasks create command");
                let _ = fs::remove_file(temp_xml_path);
                return;
            }
        };

        let _ = fs::remove_file(temp_xml_path);

        if status.success() {
            info!("Windows Task Scheduler startup entry created");
        } else {
            error!("Failed to create Task Scheduler startup entry");
        }
    }

    pub fn refresh(start_with_windows: bool, run_as_admin: bool) {
        if start_with_windows {
            if !Self::is_registered_with(run_as_admin) {
                info!(
                    run_as_admin,
                    "Reconfiguring startup task to match persisted settings"
                );
                Self::register(run_as_admin);
            }
        } else if Self::task_exists() {
            Self::unregister();
        }
    }

    /// Synchronous variant of [`Self::refresh`], for callers that must leave
    /// the task in the correct state before a process relaunch
    pub fn refresh_now(start_with_windows: bool, run_as_admin: bool) {
        if start_with_windows {
            if !Self::is_registered_with(run_as_admin) {
                info!(
                    run_as_admin,
                    "Reconfiguring startup task to match persisted settings"
                );
                Self::register_now(run_as_admin);
            }
        } else if Self::task_exists() {
            Self::unregister();
        }
    }

    pub fn unregister() {
        let status = match Command::new("schtasks")
            .args(["/Delete", "/TN", Self::TASK_NAME, "/F"])
            .status()
        {
            Ok(s) => s,
            Err(e) => {
                error!(error = %e, "Failed to execute schtasks delete command");
                return;
            }
        };

        if !status.success() {
            error!("Failed to delete startup task");
        } else {
            info!("Startup task removed");
        }
    }

    pub fn task_exists() -> bool {
        Self::query_task_xml().is_some()
    }

    pub fn is_registered() -> bool {
        let current_path = current_exe_path();
        Self::query_task_xml().is_some_and(|xml| xml.contains(&current_path))
    }

    pub fn is_registered_with(run_as_admin: bool) -> bool {
        let current_path = current_exe_path();
        Self::query_task_xml().is_some_and(|xml| {
            xml.contains(&current_path) && xml.contains(Self::run_level(run_as_admin))
        })
    }

    fn run_level(run_as_admin: bool) -> &'static str {
        if run_as_admin {
            "HighestAvailable"
        } else {
            "LeastPrivilege"
        }
    }

    fn query_task_xml() -> Option<String> {
        let output = Command::new("schtasks")
            .args(["/Query", "/TN", Self::TASK_NAME, "/XML"])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

fn current_exe_path() -> String {
    env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_level_matches_admin_preference() {
        assert_eq!(Startup::run_level(true), "HighestAvailable");
        assert_eq!(Startup::run_level(false), "LeastPrivilege");
    }
}
