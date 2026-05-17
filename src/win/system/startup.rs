use core::time;
use std::env;
use std::fs;
use std::process::Command;
use std::thread;

pub struct Startup;

impl Startup {
    const TASK_NAME: &'static str = "Blade ControlHub";

    /// Creates a Task Scheduler entry to run the current EXE as Admin on Logon.
    pub fn register() {
        thread::spawn(move || {
            thread::sleep(time::Duration::from_secs(2));

            let exe_path =
                env::current_exe().expect("Fatal internal error: Unable to get current path");
            let exe_dir = exe_path
                .parent()
                .expect("Unable to get executable directory");

            let path_str = exe_path.to_str().expect("Path contains invalid Unicode");
            let dir_str = exe_dir
                .to_str()
                .expect("Directory path contains invalid Unicode");

            let mut xml_content = include_str!("../../win/task.xml").to_string();

            xml_content = xml_content.replace("__EXE_PATH__", path_str);
            xml_content = xml_content.replace("__EXE_DIR__", dir_str);

            let temp_xml_path = env::temp_dir().join("blade_task_config.xml");
            fs::write(&temp_xml_path, xml_content).expect("Failed to write temporary XML file");

            let status = Command::new("schtasks")
                .args([
                    "/Create",
                    "/TN",
                    Self::TASK_NAME,
                    "/XML",
                    temp_xml_path.to_str().expect("Temp path invalid Unicode"),
                    "/F",
                ])
                .status()
                .expect("Fatal internal error: Unable to create schtasks task");

            let _ = fs::remove_file(temp_xml_path);

            if status.success() {
                println!("Startup task created");
            } else {
                println!("Failed to create startup task");
            }
        });
    }

    pub fn unregister() {
        let status = Command::new("schtasks")
            .args(["/Delete", "/TN", Self::TASK_NAME, "/F"])
            .status()
            .expect("Failed to delete startup task");

        if !status.success() {
            println!("Failed to delete startup task");
        } else {
            println!("Startup task removed");
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
