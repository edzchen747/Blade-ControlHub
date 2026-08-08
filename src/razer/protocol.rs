use crate::error::{AppError, AppResult};
use crate::runtime::settings_updates;
use librazer::{device::Device, packet::Packet};
use tracing::{debug, warn};

pub(crate) const HID_PACKET_ARGS_LEN: usize = 80;
const HID_COMMAND_ATTEMPTS: u8 = 3;

pub fn command(
    device: &Device,
    command: u16,
    args: &[u8],
    result_indices: Option<&[usize]>,
) -> AppResult<Vec<u8>> {
    command_with_settings_update(device, command, args, result_indices, true)
}

/// Sends a HID command without triggering a settings-state refresh. Use this
/// for high-frequency transient output such as ambient keyboard colours.
pub fn command_without_settings_update(
    device: &Device,
    command: u16,
    args: &[u8],
    result_indices: Option<&[usize]>,
) -> AppResult<Vec<u8>> {
    command_with_settings_update(device, command, args, result_indices, false)
}

fn command_with_settings_update(
    device: &Device,
    command: u16,
    args: &[u8],
    result_indices: Option<&[usize]>,
    notify_settings: bool,
) -> AppResult<Vec<u8>> {
    let mut errors: Vec<anyhow::Error> = vec![];
    for _attempt in 1..=HID_COMMAND_ATTEMPTS {
        let report = new_report(command, args)?;
        match device.send(report) {
            Ok(response) => {
                if response.get_args().len() >= args.len()
                    && response_valid(&response, args, result_indices)
                {
                    let result = response_result_bytes(&response, result_indices)?;
                    if notify_settings {
                        settings_updates::notify_settings_updated();
                    }
                    return Ok(result);
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
    Err(command_failure_error(command, args, &errors))
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

fn response_valid(response: &Packet, args: &[u8], result_indices: Option<&[usize]>) -> bool {
    let result_indices = result_indices.unwrap_or_default();
    response.get_args().iter().enumerate().zip(args.iter()).all(
        |((i, &response_byte), &arg_byte)| result_indices.contains(&i) || response_byte == arg_byte,
    )
}

fn response_result_bytes(
    response: &Packet,
    result_indices: Option<&[usize]>,
) -> AppResult<Vec<u8>> {
    result_indices
        .unwrap_or_default()
        .iter()
        .map(|&idx| {
            response.get_args().get(idx).copied().ok_or_else(|| {
                AppError::Internal(format!(
                    "Response result index {idx} is outside the HID packet payload"
                ))
            })
        })
        .collect()
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

fn command_failure_error(command: u16, args: &[u8], errors: &[anyhow::Error]) -> AppError {
    if matches!(get_first_os_error(errors), Some(1167)) {
        AppError::HardwareDisconnected
    } else {
        AppError::Protocol {
            command,
            args: args.to_vec(),
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
    fn response_valid_allows_different_result_indexes() {
        let response = packet_with_args(&[1, 5, 42]);

        assert!(response_valid(&response, &[1, 5, 0], Some(&[2])));
    }

    #[test]
    fn response_valid_allows_performance_mode_and_fan_mode_results() {
        let response = packet_with_args(&[0, 0, 6, 1]);

        assert!(response_valid(&response, &[0, 0, 0, 0], Some(&[2, 3])));
        assert_eq!(
            response_result_bytes(&response, Some(&[2, 3])).unwrap(),
            [6, 1]
        );
    }

    #[test]
    fn response_valid_allows_battery_care_level_in_first_argument() {
        let response = packet_with_args(&[208]);

        assert!(response_valid(&response, &[0], Some(&[0])));
        assert_eq!(response_result_bytes(&response, Some(&[0])).unwrap(), [208]);
    }

    #[test]
    fn response_valid_rejects_different_non_result_byte() {
        let response = packet_with_args(&[1, 4, 42]);

        assert!(!response_valid(&response, &[1, 5, 0], Some(&[2])));
    }

    #[test]
    fn response_result_bytes_are_empty_without_result_indexes() {
        let response = packet_with_args(&[1, 5, 42]);

        assert_eq!(
            response_result_bytes(&response, None).expect("valid"),
            Vec::<u8>::new()
        );
    }

    #[test]
    fn response_result_bytes_return_requested_bytes_in_order() {
        let response = packet_with_args(&[1, 5, 42, 7]);

        assert_eq!(
            response_result_bytes(&response, Some(&[2, 0, 3])).expect("valid"),
            [42, 1, 7]
        );
    }

    #[test]
    fn response_result_bytes_reject_out_of_range_index() {
        let response = packet_with_args(&[1, 5, 42]);

        assert!(response_result_bytes(&response, Some(&[usize::MAX])).is_err());
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
            command_failure_error(0x0d82, &[], &errors),
            AppError::HardwareDisconnected
        ));
    }

    #[test]
    fn command_failure_uses_protocol_error_for_non_disconnect_failures() {
        let errors = [anyhow::Error::msg("retry exhausted")];

        assert!(matches!(
            command_failure_error(0x0d82, &[0x01, 0xff], &errors),
            AppError::Protocol {
                command: 0x0d82,
                args,
                attempts: HID_COMMAND_ATTEMPTS
            } if args == [0x01, 0xff]
        ));
    }
}
