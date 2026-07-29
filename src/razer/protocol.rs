use crate::error::{AppError, AppResult};
use librazer::{device::Device, packet::Packet};
use tracing::{debug, warn};

pub(crate) const HID_PACKET_ARGS_LEN: usize = 80;
const HID_COMMAND_ATTEMPTS: u8 = 3;

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
    for _attempt in 1..=HID_COMMAND_ATTEMPTS {
        let report = new_report(command, args)?;
        match device.send(report) {
            Ok(response) => {
                if response.get_args().len() >= args.len()
                    && response_valid(&response, args, result_idx)
                {
                    return response_result_byte(&response, result_idx);
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
    Err(command_failure_error(command, &errors))
}

fn new_report(command: u16, args: &[u8]) -> AppResult<Packet> {
    if args.len() > HID_PACKET_ARGS_LEN {
        return Err(AppError::Internal(format!(
            "HID packet args too long: {} > {}",
            args.len(),
            HID_PACKET_ARGS_LEN
        )));
    }

    Ok(Packet::new(command, args))
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
        .zip(args.iter())
        .all(|((i, &response_byte), &arg_byte)| i == skip_idx || response_byte == arg_byte)
}

fn response_result_byte(response: &Packet, idx: Option<usize>) -> AppResult<u8> {
    let Some(idx) = idx else {
        return Ok(0);
    };

    response.get_args().get(idx).copied().ok_or_else(|| {
        AppError::Internal(format!(
            "Response result index {idx} is outside the HID packet payload"
        ))
    })
}

fn get_first_os_error(errors: &[anyhow::Error]) -> Option<i32> {
    errors.iter().find_map(|err| {
        err.downcast_ref::<std::io::Error>()
            .and_then(std::io::Error::raw_os_error)
            .or_else(|| {
                err.source().and_then(|source| {
                    source
                        .downcast_ref::<std::io::Error>()
                        .and_then(std::io::Error::raw_os_error)
                })
            })
    })
}

fn command_failure_error(command: u16, errors: &[anyhow::Error]) -> AppError {
    if matches!(get_first_os_error(errors), Some(1167)) {
        AppError::HardwareDisconnected
    } else {
        AppError::Protocol {
            command,
            attempts: HID_COMMAND_ATTEMPTS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet_with_args(args: &[u8]) -> Packet {
        Packet::new(0x0303, args)
    }

    #[test]
    fn new_report_rejects_args_larger_than_external_payload() {
        let oversized = [0u8; HID_PACKET_ARGS_LEN + 1];

        assert!(new_report(0x0303, &oversized).is_err());
    }

    #[test]
    fn response_valid_allows_different_result_index() {
        let response = packet_with_args(&[1, 5, 42]);

        assert!(response_valid(&response, &[1, 5, 0], Some(2)));
    }

    #[test]
    fn response_valid_rejects_different_non_result_byte() {
        let response = packet_with_args(&[1, 4, 42]);

        assert!(!response_valid(&response, &[1, 5, 0], Some(2)));
    }

    #[test]
    fn response_result_byte_defaults_to_zero_without_result_index() {
        let response = packet_with_args(&[1, 5, 42]);

        assert_eq!(response_result_byte(&response, None).expect("valid"), 0);
    }

    #[test]
    fn response_result_byte_returns_requested_byte() {
        let response = packet_with_args(&[1, 5, 42]);

        assert_eq!(response_result_byte(&response, Some(2)).expect("valid"), 42);
    }

    #[test]
    fn response_result_byte_rejects_out_of_range_index() {
        let response = packet_with_args(&[1, 5, 42]);

        assert!(response_result_byte(&response, Some(usize::MAX)).is_err());
    }

    #[test]
    fn get_first_os_error_reads_root_io_error() {
        let errors = [anyhow::Error::new(std::io::Error::from_raw_os_error(1167))];

        assert_eq!(get_first_os_error(&errors), Some(1167));
    }

    #[test]
    fn command_failure_maps_device_disconnect_without_restarting_process() {
        let errors = [anyhow::Error::new(std::io::Error::from_raw_os_error(1167))];

        assert!(matches!(
            command_failure_error(0x0d82, &errors),
            AppError::HardwareDisconnected
        ));
    }

    #[test]
    fn command_failure_uses_protocol_error_for_non_disconnect_failures() {
        let errors = [anyhow::Error::msg("retry exhausted")];

        assert!(matches!(
            command_failure_error(0x0d82, &errors),
            AppError::Protocol {
                command: 0x0d82,
                attempts: HID_COMMAND_ATTEMPTS
            }
        ));
    }
}
