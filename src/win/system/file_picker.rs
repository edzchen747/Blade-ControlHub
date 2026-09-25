//! The Browse button behind the application picker.
//!
//! This is a Win32 `IFileOpenDialog` rather than a Tauri dialog plugin: the COM
//! machinery it needs is already enabled for the Start menu index, so a plugin
//! would add a dependency and a capability entry to reach the same dialog. It
//! must run on a worker thread — it blocks until the user answers — and it is
//! given the settings window as its owner so it cannot end up behind it.

use std::path::PathBuf;

use tracing::warn;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, CoUninitialize,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH};
use windows::core::PCWSTR;

/// Asks the user for an application. `None` means they cancelled.
///
/// `owner` is the settings window's `HWND` as a raw handle value, or 0 for an
/// unowned dialog.
pub fn pick_executable(owner: isize) -> Option<PathBuf> {
    let initialised = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    let picked = show_dialog(owner);
    if initialised {
        unsafe { CoUninitialize() };
    }
    picked
}

fn show_dialog(owner: isize) -> Option<PathBuf> {
    let dialog: IFileOpenDialog =
        match unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) } {
            Ok(dialog) => dialog,
            Err(error) => {
                warn!(%error, "Could not open the file picker");
                return None;
            }
        };

    let programs = wide("Programs");
    let programs_spec = wide("*.exe;*.lnk;*.bat;*.cmd;*.ps1");
    let all = wide("All files");
    let all_spec = wide("*.*");
    let filters = [
        COMDLG_FILTERSPEC {
            pszName: PCWSTR(programs.as_ptr()),
            pszSpec: PCWSTR(programs_spec.as_ptr()),
        },
        COMDLG_FILTERSPEC {
            pszName: PCWSTR(all.as_ptr()),
            pszSpec: PCWSTR(all_spec.as_ptr()),
        },
    ];

    let title = wide("Choose an application");
    unsafe {
        let _ = dialog.SetFileTypes(&filters);
        let _ = dialog.SetTitle(PCWSTR(title.as_ptr()));
    }

    // A cancelled dialog reports an error, which is not worth logging.
    if unsafe { dialog.Show(HWND(owner as *mut std::ffi::c_void)) }.is_err() {
        return None;
    }

    let item = unsafe { dialog.GetResult() }.ok()?;
    let name = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }.ok()?;
    let path = unsafe { name.to_string() }.ok().map(PathBuf::from);
    unsafe { CoTaskMemFree(Some(name.0 as *const _)) };
    path
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
