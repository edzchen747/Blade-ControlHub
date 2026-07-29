use std::{os::windows::process::CommandExt, process::Command};

use crate::ui::{app::app, app_events::OsdEvent};

/// Disables and enables Nvidia GPUs using powershell command
pub fn cycle_gpu() {
    std::thread::spawn(|| {
        let script = r#"
            $d = Get-PnpDevice | Where-Object {$_.FriendlyName -like '*NVIDIA GeForce*'}
            $d | Disable-PnpDevice -Confirm:$false
            $d | Enable-PnpDevice -Confirm:$false
        "#;

        const CREATE_NO_WINDOW: u32 = 0x08000000;

        app(OsdEvent::CloseGPUApps(false).into());
        let _ = Command::new("powershell")
            .args([
                "-NoProfile",
                "-WindowStyle",
                "Hidden", // <-- 2. Forces PowerShell's window system to remain invisible
                "-Command",
                script,
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        app(OsdEvent::CloseGPUApps(true).into());
    });
}
