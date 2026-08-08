//! Process elevation helpers.
//!
//! The app runs as the invoking user by default (`asInvoker` manifest).
//! Elevation is requested explicitly only when the "Start with administrator
//! privileges" setting is enabled: on launch the process relaunches itself
//! elevated (UAC), and enabling the setting relaunches the app elevated.
//! Disabling the setting keeps the current session's privileges untouched and
//! only downgrades the startup task, so the next manual launch is unelevated.

use std::io;
use std::mem;
use std::ptr;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_FAILED};
use windows_sys::Win32::System::Threading::WaitForSingleObject;
use windows_sys::Win32::UI::Shell::{IsUserAnAdmin, SHELLEXECUTEINFOW, ShellExecuteExW};

const INFINITE: u32 = u32::MAX;

pub fn is_elevated() -> bool {
    unsafe { IsUserAnAdmin() != 0 }
}

pub fn spawn_self_elevated() -> io::Result<()> {
    let (file, parameters) = current_exe_and_args()?;
    spawn_elevated(&file, &parameters)
}

/// Blocks until the elevation request is answered.
pub fn spawn_elevated(application: &str, parameters: &str) -> io::Result<()> {
    spawn_elevated_impl(application, parameters, false).map(|_| ())
}

/// Returns the process handle. Blocks until the elevation request
/// is answered; the handle is valid only when the user accepted.
pub fn spawn_elevated_process(application: &str, parameters: &str) -> io::Result<HANDLE> {
    spawn_elevated_impl(application, parameters, true)
}

pub fn wait_for_process(process: HANDLE) -> io::Result<()> {
    let wait_result = unsafe { WaitForSingleObject(process, INFINITE) };
    unsafe {
        let _ = CloseHandle(process);
    }
    if wait_result == WAIT_FAILED {
        return Err(os_error("waiting for elevated process failed"));
    }
    Ok(())
}

fn spawn_elevated_impl(
    application: &str,
    parameters: &str,
    keep_process_handle: bool,
) -> io::Result<HANDLE> {
    const SEE_MASK_NOCLOSEPROCESS: u32 = 0x0000_0040;

    let verb = wide("runas");
    let file = wide(application);
    let parameters = wide(parameters);

    let mut info: SHELLEXECUTEINFOW = unsafe { mem::zeroed() };
    info.cbSize = mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = if keep_process_handle {
        SEE_MASK_NOCLOSEPROCESS
    } else {
        0
    };
    info.hwnd = 0;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = parameters.as_ptr();
    info.lpDirectory = ptr::null();
    info.nShow = 0; // SW_HIDE: the parent must not flash any window

    let launched = unsafe { ShellExecuteExW(&mut info) };
    if launched == 0 {
        return Err(io::Error::last_os_error());
    }
    let process = info.hProcess;
    if keep_process_handle && process == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(process)
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn current_exe_and_args() -> io::Result<(String, String)> {
    let exe = std::env::current_exe()?;
    let file = exe.to_string_lossy().to_string();
    let args = std::env::args()
        .skip(1)
        .map(|arg| quote_arg(&arg))
        .collect::<Vec<_>>()
        .join(" ");
    Ok((file, args))
}

fn quote_arg(arg: &str) -> String {
    if arg.contains(' ') || arg.contains('\t') {
        format!("\"{}\"", arg.replace('"', "\\\""))
    } else {
        arg.to_string()
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// Error with the Win32 code captured at the failing call site.
fn os_error(context: &str) -> io::Error {
    let code = io::Error::last_os_error();
    io::Error::new(code.kind(), format!("{context} ({code})"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_arg_quotes_only_args_with_whitespace() {
        assert_eq!(quote_arg("--silent"), "--silent");
        assert_eq!(quote_arg("--debug"), "--debug");
        assert_eq!(
            quote_arg("C:\\Program Files\\Blade\\blade-controlhub.exe"),
            "\"C:\\Program Files\\Blade\\blade-controlhub.exe\""
        );
    }

    #[test]
    fn current_exe_and_args_preserve_the_executable_path() {
        let (file, _) = current_exe_and_args().expect("exe and args must build");
        let exe = std::env::current_exe().expect("current exe must resolve");

        assert!(file.contains(&exe.to_string_lossy().to_string()));
    }
}
