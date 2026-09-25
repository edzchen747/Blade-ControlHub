//! Starting applications and shell commands on behalf of a key binding.
//!
//! Two things make this more than a `Command::spawn`. A child inherits the
//! parent's token, so launching from an elevated ControlHub would silently hand
//! the user's browser or game administrator rights; when this process is
//! elevated the launch is handed to Explorer instead, which runs at the user's
//! own integrity level. And nothing launched here is ever waited on — the
//! caller is a key-binding worker, not a supervisor.

use std::io;
use std::os::windows::process::CommandExt;
use std::process::Command;

use tracing::warn;

use crate::win::system::elevation::{is_elevated, quote_arg, shell_open};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

/// Starts an application, shortcut or document. `path` may be an `.exe`, a
/// Start-menu `.lnk` or anything else the shell knows how to open.
pub fn launch_app(path: &str, args: &str) -> io::Result<()> {
    let path = path.trim();
    if path.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "no application path",
        ));
    }

    if is_elevated() {
        return launch_via_explorer(path, args);
    }
    shell_open(path, args.trim())
}

/// Runs a command line through `cmd /C`, detached and without a console
/// window. Shell syntax is the point here — the user typed a command, not an
/// executable and an argument vector.
pub fn run_command(command: &str) -> io::Result<()> {
    let command = command.trim();
    if command.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty command"));
    }

    Command::new("cmd")
        .args(["/C", command])
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .map(|_| ())
}

/// Explorer is already running as the user, so a launch it performs drops the
/// administrator token this process may be holding. Explorer takes the target
/// as a single argument, so any arguments of our own are appended to it.
fn launch_via_explorer(path: &str, args: &str) -> io::Result<()> {
    let args = args.trim();
    let target = if args.is_empty() {
        quote_arg(path)
    } else {
        format!("{} {args}", quote_arg(path))
    };

    let spawned = Command::new("explorer.exe")
        .arg(&target)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();

    match spawned {
        Ok(_) => Ok(()),
        Err(error) => {
            // Better a launch the user did not expect to be elevated than no
            // launch at all; the settings window warns about this case.
            warn!(%error, "Explorer launch failed; starting the application elevated instead");
            shell_open(path, args)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An incomplete row is dropped before it is ever bound, but these are the
    /// last line of defence: a blank path must be refused rather than handed to
    /// the shell, which would open a folder window or the user's home directory.
    #[test]
    fn a_blank_application_path_is_refused() {
        for path in ["", "   ", "\t\n"] {
            let error = launch_app(path, "").expect_err("a blank path must not launch");
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        }
    }

    #[test]
    fn a_blank_command_is_refused() {
        for command in ["", "  ", "\n"] {
            let error = run_command(command).expect_err("a blank command must not run");
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        }
    }
}
