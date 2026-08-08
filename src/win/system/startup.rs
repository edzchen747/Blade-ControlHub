use std::env;
use std::fs;
use std::io;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, ExitStatus};
use std::thread;
use tracing::{error, info, warn};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct Startup;

impl Startup {
    const TASK_NAME: &'static str = "Blade ControlHub";

    /// Creates a Task Scheduler entry to run the current EXE on Logon, at the
    /// requested privilege level (elevated or not). Runs on a background
    /// thread so the caller (device worker) is not blocked.
    pub fn register(run_as_admin: bool) {
        Self::spawn_background("blade-startup-register", move || {
            Self::register_now(run_as_admin);
        });
    }

    /// Synchronous variant of [`Self::register`], for callers that must leave
    /// the task in the correct state before a process relaunch.
    pub fn register_now(run_as_admin: bool) {
        let Some((temp_xml_path, temp_xml_str)) = Self::write_task_xml(run_as_admin) else {
            return;
        };

        let args = [
            "/Create",
            "/TN",
            Self::TASK_NAME,
            "/XML",
            &temp_xml_str,
            "/F",
        ];
        if Self::run_schtasks(&args).is_ok_and(|s| s.success()) {
            info!("Windows Task Scheduler startup entry created");
        } else if Self::retry_elevated(&args) && Self::is_registered_with(run_as_admin) {
            info!("Windows Task Scheduler startup entry created (elevated)");
        } else {
            error!("Failed to create Task Scheduler startup entry");
        }

        let _ = fs::remove_file(&temp_xml_path);
    }

    /// Runs on a background thread so the caller (device worker) is not
    /// blocked by schtasks.
    pub fn unregister() {
        Self::spawn_background("blade-startup-unregister", || {
            Self::unregister_now();
        });
    }

    /// Synchronous variant of [`Self::unregister`]. When the app is not
    /// elevated and the direct attempt fails, requests elevation (UAC) to
    /// delete the task.
    pub fn unregister_now() {
        let args = ["/Delete", "/TN", Self::TASK_NAME, "/F"];
        if Self::run_schtasks(&args).is_ok_and(|s| s.success()) {
            info!("Startup task removed");
        } else if Self::retry_elevated(&args) && !Self::task_exists() {
            info!("Startup task removed (elevated)");
        } else {
            error!("Failed to delete startup task");
        }
    }

    /// Makes the Task Scheduler entry match the persisted start-with-windows
    /// flag and the current admin preference: registers (with the correct run
    /// level) when enabled, removes the task when disabled.
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
            Self::unregister_now();
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
            if !xml.contains(&current_path) {
                return false;
            }
            if run_as_admin {
                xml.contains("HighestAvailable")
            } else {
                xml.contains("LeastPrivilege") || !xml.contains("RunLevel")
            }
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
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn run_schtasks(args: &[&str]) -> io::Result<ExitStatus> {
        Command::new("schtasks")
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .status()
    }

    fn retry_elevated(args: &[&str]) -> bool {
        if crate::win::system::elevation::is_elevated() {
            return false;
        }

        let command_line = args
            .iter()
            .map(|arg| quote_arg(arg))
            .collect::<Vec<_>>()
            .join(" ");
        match crate::win::system::elevation::spawn_elevated_process("schtasks.exe", &command_line) {
            Ok(process) => {
                let _ = crate::win::system::elevation::wait_for_process(process);
                true
            }
            Err(error) => {
                warn!(%error, "Elevation for task scheduler change was declined or failed");
                false
            }
        }
    }

    fn write_task_xml(run_as_admin: bool) -> Option<(PathBuf, String)> {
        let exe_path = env::current_exe().ok()?;
        let exe_dir = exe_path.parent()?;
        let path_str = exe_path.to_str()?;
        let dir_str = exe_dir.to_str()?;

        let mut xml_content = include_str!("../../win/task.xml").to_string();
        xml_content = xml_content.replace("__EXE_PATH__", path_str);
        xml_content = xml_content.replace("__EXE_DIR__", dir_str);
        xml_content = xml_content.replace("__RUN_LEVEL__", Self::run_level(run_as_admin));

        let temp_xml_path = env::temp_dir().join("blade_task_config.xml");
        if let Err(e) = fs::write(&temp_xml_path, xml_content) {
            error!(error = %e, "Failed to write temporary task XML file");
            return None;
        }

        let temp_xml_str = temp_xml_path.to_str()?.to_string();
        Some((temp_xml_path, temp_xml_str))
    }

    fn spawn_background(name: &'static str, job: impl FnOnce() + Send + 'static) {
        if let Err(error) = thread::Builder::new().name(name.to_string()).spawn(job) {
            warn!(%error, "Failed to start startup task thread");
        }
    }
}

fn current_exe_path() -> String {
    env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn quote_arg(arg: &str) -> String {
    if arg.contains(' ') || arg.contains('\t') {
        format!("\"{}\"", arg.replace('"', "\\\""))
    } else {
        arg.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_level_matches_admin_preference() {
        assert_eq!(Startup::run_level(true), "HighestAvailable");
        assert_eq!(Startup::run_level(false), "LeastPrivilege");
    }

    #[test]
    fn quote_arg_quotes_task_arguments_with_spaces() {
        assert_eq!(quote_arg("Blade ControlHub"), "\"Blade ControlHub\"");
        assert_eq!(quote_arg("/F"), "/F");
    }
}
