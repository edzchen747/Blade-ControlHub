use librazer::{device::Device, packet::Packet};
use std::{thread, time::Duration};

/// Sends a HID command to the device with automatic retry logic (up to 3 attempts).
///
/// If `result_idx` is `Some(idx)`, validates the response and returns the byte at
/// that index. If `None`, returns `0` on success.
/// Returns `0` if all attempts fail.
pub fn command(device: &Device, command: u16, args: &[u8], result_idx: Option<usize>) -> u8 {
    for attempt in 1..=3 {
        let report = Packet::new(command, args);
        match device.send(report) {
            Ok(response) => {
                if response.get_args().len() >= args.len()
                    && response_valid(&response, args, result_idx)
                {
                    return result_idx.map(|idx| response.get_args()[idx]).unwrap_or(0);
                } else {
                    println!("Error: Response invalid");
                }
            }
            Err(err) => println!("{:?}, command: {:#06x} {:?}", err, command, args),
        };
        thread::sleep(Duration::from_millis(100 * attempt));
    }
    println!(
        "Command failed 3 times, skipping ({:#06x} {:?})",
        command, args
    );
    0
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
