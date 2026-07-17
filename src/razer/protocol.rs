use crate::{
    error::{AppError, AppResult},
    utils::reload::restart_app,
};
use librazer::{device::Device, packet::Packet};
use tracing::{debug, warn};

/// Sends a HID command to the device with automatic retry logic (up to 3 attempts).
///
/// If `result_idx` is `Some(idx)`, validates the response and returns the byte at
/// that index. If `None`, returns `Ok(0)` on success.
/// Returns `Err` if all attempts fail or response is invalid.
pub fn command(
    device: &Device,
    command: u16,
    args: &[u8],
    result_idx: Option<usize>,
) -> AppResult<u8> {
    let mut errors: Vec<anyhow::Error> = vec![];
    for _attempt in 1..=3 {
        let report = Packet::new(command, args);
        match device.send(report) {
            Ok(response) => {
                if response.get_args().len() >= args.len()
                    && response_valid(&response, args, result_idx)
                {
                    return Ok(result_idx.map(|idx| response.get_args()[idx]).unwrap_or(0));
                } else {
                    debug!(command = command, "Response validation failed, retrying");
                    continue;
                }
            }
            Err(err) => {
                warn!(command = command, error = ?err, "HID send failed, retrying");
                errors.push(err)
            }
        };
    }
    if let Some(code) = get_first_os_error(&errors) {
        match code {
            1167 => restart_app(1), // The device is not connected
            _ => (),
        }
    }
    Err(AppError::Protocol {
        command,
        attempts: 3,
    })
}

/// Validates that a response packet matches the sent arguments,
/// skipping the result index position.
fn response_valid(response: &Packet, args: &[u8], idx: Option<usize>) -> bool {
    let Some(skip_idx) = idx else {
        return true;
    };
    response
        .get_args()
        .iter()
        .enumerate()
        .take(args.len())
        .filter(|&(i, _)| i != skip_idx)
        .all(|(i, &byte)| byte == args[i])
}

fn get_first_os_error(errors: &[anyhow::Error]) -> Option<i32> {
    errors.iter().find_map(|err| {
        let os_error = err.source().and_then(|e| {
            e.downcast_ref::<std::io::Error>()
                .map(|io_err| io_err.raw_os_error())
        });

        os_error.flatten()
    })
}
