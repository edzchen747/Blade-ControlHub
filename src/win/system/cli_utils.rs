use std::process::Command;

use crate::ui::{app::app, app_events::OsdEvent};
use crate::win::system::elevation::{is_elevated, spawn_elevated_process, wait_for_process};

/// Disables and enables Nvidia GPUs using powershell command.
pub fn cycle_gpu() {
    let spawn_result = std::thread::Builder::new()
        .name("blade-cycle-gpu".to_string())
        .spawn(|| {
            let script = r#"
            $d = Get-PnpDevice | Where-Object {$_.FriendlyName -like '*NVIDIA GeForce*'}
            $d | Disable-PnpDevice -Confirm:$false
            $d | Enable-PnpDevice -Confirm:$false
        "#;

            if is_elevated() {
                app(OsdEvent::CloseGPUApps(false).into());
                let _ = run_gpu_script(script);
                app(OsdEvent::CloseGPUApps(true).into());
                return;
            }

            let parameters = format!(
                "-NoProfile -WindowStyle Hidden -Command \"{}\"",
                script.replace('"', "\\\"")
            );
            match spawn_elevated_process("powershell.exe", &parameters) {
                Ok(process) => {
                    app(OsdEvent::CloseGPUApps(false).into());
                    if let Err(error) = wait_for_process(process) {
                        tracing::warn!(%error, "Failed to wait for elevated GPU cycle");
                    }
                    app(OsdEvent::CloseGPUApps(true).into());
                }
                Err(error) => {
                    tracing::warn!(%error, "Failed to request elevation for GPU cycle");
                }
            }
        });

    if let Err(error) = spawn_result {
        tracing::warn!(%error, "Failed to start GPU cycle worker thread");
    }
}

fn run_gpu_script(script: &str) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    Command::new("powershell")
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden", // <-- 2. Forces PowerShell's window system to remain invisible
            "-Command",
            script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(drop)
}
