use crate::error::{AppError, AppResult};
use librazer::{device::Device, packet::Packet};
use std::{thread, time::Duration};
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
    for attempt in 1..=3 {
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
            Err(err) => warn!(command = command, error = ?err, "HID send failed, retrying"),
        };
        thread::sleep(Duration::from_millis(100 * attempt));
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
