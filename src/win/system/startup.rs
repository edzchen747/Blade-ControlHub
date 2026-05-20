use core::time;
use std::env;
use std::fs;
use std::process::Command;
use std::thread;
use tracing::{error, info};

pub struct Startup;

impl Startup {
    const TASK_NAME: &'static str = "Blade ControlHub";

    /// Creates a Task Scheduler entry to run the current EXE as Admin on Logon.
    pub fn register() {
        thread::spawn(move || {
            thread::sleep(time::Duration::from_secs(2));

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
        });
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

    pub fn is_registered() -> bool {
        let current_path = match env::current_exe() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => return false,
        };

        let output = Command::new("schtasks")
            .args(["/Query", "/TN", Self::TASK_NAME, "/XML"])
            .output();

        if let Ok(out) = output
            && out.status.success()
        {
            let xml_content = String::from_utf8_lossy(&out.stdout);
            return xml_content.contains(&current_path);
        }
        false
    }
}
