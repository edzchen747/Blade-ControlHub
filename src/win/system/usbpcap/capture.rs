//! Native USBPcap capture for Command Lab.
//!
//! Mirrors the USBPcapCMD flow: enumerate the filter whose root hub hosts the
//! Razer EC device (VID 0x1532), open the filter with GENERIC_READ/GENERIC_WRITE
//! (requires administrator privileges), set up the kernel capture buffer, then
//! read packets for the countdown duration and write only the frames matching
//! `len(frame) == 126` to a pcap file next to the executable.
//!
//! USBPcap has no BPF engine, so the frame-length filter is applied in this
//! reader: a frame's length is `headerLen + dataLength` from the packed
//! `USBPCAP_BUFFER_PACKET_HEADER`, which equals what Wireshark reports as
//! `frame.len` for the USBPcap link type (249).
//!
//! Each captured frame is a control transfer whose data fragment holds one
//! Razer EC command (`extract_command`), counted live so the UI can show the
//! number of captured commands next to the record button.

use std::io::{self, Write};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tracing::{info, warn};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ACCESS_DENIED, ERROR_IO_PENDING, GENERIC_READ,
    GENERIC_WRITE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_OVERLAPPED, FILE_SHARE_WRITE, OPEN_EXISTING, ReadFile,
};
use windows_sys::Win32::System::IO::{
    CancelIo, DeviceIoControl, GetOverlappedResult, OVERLAPPED,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetExitCodeProcess, ResetEvent, TerminateProcess, WaitForSingleObject,
};
use windows_sys::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NO_CONSOLE, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    ShellExecuteExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

use super::{
    HUB_SYMLINK_BUFFER_WORDS, IOCTL_USBPCAP_GET_HUB_SYMLINK, MAX_USBPCAP_INTERFACES, RAZER_VID,
    usbpcap_interface_name,
};

/// Command Lab filter: only frames whose total length is exactly this are kept.
pub const COMMAND_LAB_FRAME_LEN: u32 = 126;
/// How long a Command Lab recording captures, matching the OSD countdown.
pub const COMMAND_LAB_CAPTURE_DURATION: Duration = Duration::from_secs(5);

/// CTL_CODE(FILE_DEVICE_UNKNOWN, 0x800, METHOD_BUFFERED, FILE_READ_ACCESS).
const IOCTL_USBPCAP_SETUP_BUFFER: u32 = 0x0022_6000;
/// CTL_CODE(FILE_DEVICE_UNKNOWN, 0x801, METHOD_BUFFERED, FILE_READ_ACCESS | FILE_WRITE_ACCESS).
const IOCTL_USBPCAP_START_FILTERING: u32 = 0x0022_E004;
/// CTL_CODE(FILE_DEVICE_UNKNOWN, 0x804, METHOD_BUFFERED, FILE_READ_ACCESS).
const IOCTL_USBPCAP_SET_SNAPLEN_SIZE: u32 = 0x0022_6010;
/// CTL_CODE(FILE_DEVICE_USB, USB_GET_NODE_INFORMATION, METHOD_BUFFERED, FILE_ANY_ACCESS).
const IOCTL_USB_GET_NODE_INFORMATION: u32 = 0x0022_0408;
/// CTL_CODE(FILE_DEVICE_USB, USB_GET_NODE_CONNECTION_INFORMATION, METHOD_BUFFERED, FILE_ANY_ACCESS).
const IOCTL_USB_GET_NODE_CONNECTION_INFORMATION: u32 = 0x0022_040C;

const USBPCAP_SNAPLEN: u32 = 65535;
const USBPCAP_BUFFER_LEN: u32 = 1024 * 1024;
const READ_POLL_INTERVAL: Duration = Duration::from_millis(50);
const USBPCAP_HEADER_LEN_MIN: usize = 27;
const PCAP_GLOBAL_HEADER_LEN: usize = 24;
const PCAP_RECORD_HEADER_LEN: usize = 16;
/// USB setup packet size preceding the control-transfer data fragment.
const USB_SETUP_PACKET_LEN: usize = 8;
/// Data fragment layout: [5-byte report prefix][1-byte argument count]
/// [2-byte command][arguments].
const FRAGMENT_PREFIX_LEN: usize = 5;
const FRAGMENT_COMMAND_OFFSET: usize = 6;
const FRAGMENT_ARGS_OFFSET: usize = 8;

/// USB_NODE_INFORMATION (packed): bNumberOfPorts of USB_HUB_DESCRIPTOR.
const NODE_INFORMATION_PORT_COUNT_OFFSET: usize = 6;
/// USB_NODE_CONNECTION_INFORMATION (packed) byte offsets.
const NODE_CONNECTION_INDEX_OFFSET: usize = 0;
const NODE_CONNECTION_ID_VENDOR_OFFSET: usize = 12;
const NODE_CONNECTION_STATUS_OFFSET: usize = 31;
/// USB_CONNECTION_STATUS::DeviceConnected.
const USB_CONNECTION_DEVICE_CONNECTED: u32 = 1;

const USBPCAP_ADDRESS_FILTER_LEN: usize = 17;

/// A Razer EC command parsed from a captured frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedCommand {
    /// Command identifier, e.g. `0x0004`.
    pub command: u16,
    /// Command arguments, exactly the count reported by the frame.
    pub args: Vec<u8>,
}

/// Parses one captured 126-byte frame into a Razer EC command. The relevant
/// bytes are the control-transfer data fragment: after the USBPcap header
/// (length from its `headerLen` field) and the 8-byte USB setup packet, the
/// fragment is `[5-byte report prefix][1-byte argument count][2-byte command]
/// [arguments...]`. Trailing zero padding beyond the argument count is not
/// treated as arguments.
pub fn extract_command(frame: &[u8]) -> Option<CapturedCommand> {
    let header_len = u16::from_le_bytes(frame.get(0..2)?.try_into().ok()?) as usize;
    let fragment = frame.get(header_len + USB_SETUP_PACKET_LEN..)?;
    if fragment.len() < FRAGMENT_ARGS_OFFSET {
        return None;
    }
    let arg_count = fragment[FRAGMENT_PREFIX_LEN] as usize;
    if fragment.len() < FRAGMENT_ARGS_OFFSET + arg_count {
        return None;
    }
    Some(CapturedCommand {
        command: u16::from_be_bytes(
            fragment[FRAGMENT_COMMAND_OFFSET..FRAGMENT_COMMAND_OFFSET + 2]
                .try_into()
                .expect("command slice has fixed length"),
        ),
        args: fragment[FRAGMENT_ARGS_OFFSET..FRAGMENT_ARGS_OFFSET + arg_count].to_vec(),
    })
}

/// A running Command Lab capture. The capture is prepared synchronously in
/// `start`. When the process is not elevated, the USBPcap filter open fails
/// with access denied; `start` then requests administrator privileges (UAC)
/// and launches an elevated capture process that writes the same output file
/// and a count sidecar. In both cases packets are captured for the countdown
/// duration and `captured_count`/`stop` report the parsed command count.
pub struct CommandLabCapture {
    inner: CaptureInner,
}

enum CaptureInner {
    Native {
        cancel: Arc<AtomicBool>,
        read_thread: Option<JoinHandle<()>>,
        commands: Arc<AtomicU64>,
    },
    Elevated {
        process: windows_sys::Win32::Foundation::HANDLE,
        output_path: PathBuf,
        commands: Arc<AtomicU64>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureError {
    /// The USBPcap filter could not be opened because the process is not
    /// elevated; a UAC elevation request can unlock the capture.
    AccessDenied,
    /// Capture is not possible (driver missing, no Razer hub, ioctl failure).
    Unavailable,
}

impl CommandLabCapture {
    /// Starts the capture. When the current process is not elevated, this
    /// blocks until the UAC elevation request is answered (the elevated
    /// capture process runs the actual capture); returns `None` when the
    /// request was declined or the capture cannot start at all.
    pub fn start() -> Option<Self> {
        Self::start_at(&command_lab_capture_output_path()?)
    }

    /// Starts the capture writing to exactly `output_path`.
    fn start_at(output_path: &Path) -> Option<Self> {
        if super::detect::usbpcap_available_interfaces() == 0 {
            warn!("Command Lab capture unavailable: USBPcap has no usable interfaces");
            return None;
        }
        match start_native(output_path) {
            Ok(capture) => Some(capture),
            Err(CaptureError::AccessDenied) => start_elevated_capture(output_path),
            Err(CaptureError::Unavailable) => None,
        }
    }

    /// Number of commands captured so far (live during recording).
    pub fn captured_count(&self) -> u32 {
        match &self.inner {
            CaptureInner::Native { commands, .. } => commands.load(Ordering::SeqCst) as u32,
            CaptureInner::Elevated {
                output_path, commands, ..
            } => {
                if let Ok(text) = std::fs::read_to_string(count_sidecar_path(output_path))
                    && let Ok(count) = text.trim().parse::<u64>()
                {
                    commands.store(count, Ordering::SeqCst);
                }
                commands.load(Ordering::SeqCst) as u32
            }
        }
    }

    /// Stops the capture and returns the total number of captured commands.
    pub fn stop(&mut self) -> u32 {
        match &mut self.inner {
            CaptureInner::Native {
                cancel,
                read_thread,
                commands,
            } => {
                cancel.store(true, Ordering::SeqCst);
                if let Some(thread) = read_thread.take()
                    && thread.thread().id() != std::thread::current().id()
                    && thread.join().is_err()
                {
                    warn!("Command Lab capture thread panicked");
                }
                commands.load(Ordering::SeqCst) as u32
            }
            CaptureInner::Elevated {
                process,
                output_path,
                commands,
            } => {
                // The child finishes its own countdown window; terminate it
                // when it lingers (for example on an early cancel).
                let wait = unsafe { WaitForSingleObject(*process, 1000) };
                if wait == WAIT_TIMEOUT {
                    unsafe { TerminateProcess(*process, 0) };
                    unsafe { WaitForSingleObject(*process, 1000) };
                }
                unsafe { CloseHandle(*process) };
                if let Ok(text) = std::fs::read_to_string(count_sidecar_path(output_path))
                    && let Ok(count) = text.trim().parse::<u64>()
                {
                    commands.store(count, Ordering::SeqCst);
                }
                commands.load(Ordering::SeqCst) as u32
            }
        }
    }

    /// Test seam: a capture that reports zero commands without touching
    /// hardware, so countdown tests stay hermetic.
    #[cfg(test)]
    pub(crate) fn dummy() -> Self {
        Self {
            inner: CaptureInner::Native {
                cancel: Arc::new(AtomicBool::new(false)),
                read_thread: None,
                commands: Arc::new(AtomicU64::new(0)),
            },
        }
    }
}

/// Standalone elevated capture process entry. Captures for the countdown
/// duration, updating the count sidecar while running, then exits `0` with
/// the final count written. Exits `1` when the capture could not start.
pub fn run_command_lab_capture_process(output_path: &std::path::Path) -> i32 {
    let Some(mut capture) = CommandLabCapture::start_at(output_path) else {
        warn!("Elevated Command Lab capture could not start");
        return 1;
    };
    info!(path = %output_path.display(), "Elevated Command Lab capture started");
    let deadline = Instant::now() + COMMAND_LAB_CAPTURE_DURATION;
    while Instant::now() < deadline {
        write_count_sidecar(output_path, capture.captured_count());
        std::thread::sleep(ELEVATED_SIDECAR_INTERVAL);
    }
    let count = capture.stop();
    write_count_sidecar(output_path, count);
    info!(count, "Elevated Command Lab capture finished");
    0
}

/// Starts the capture in-process; requires administrator privileges.
fn start_native(output_path: &Path) -> Result<CommandLabCapture, CaptureError> {
    let target = find_razer_usbpcap_filter()?;
    let filter = open_and_configure_filter(&target)?;
    let output = std::fs::File::create(output_path).map_err(|error| {
        warn!(%error, path = %output_path.display(), "Failed to create Command Lab capture file");
        CaptureError::Unavailable
    })?;

    let cancel = Arc::new(AtomicBool::new(false));
    let commands = Arc::new(AtomicU64::new(0));
    let thread_cancel = cancel.clone();
    let thread_commands = commands.clone();
    let path_log = output_path.display().to_string();
    let read_thread = match std::thread::Builder::new()
        .name("blade-command-lab-capture".to_string())
        .spawn(move || {
            let stats = read_and_filter(
                filter,
                output,
                COMMAND_LAB_CAPTURE_DURATION,
                &thread_cancel,
                COMMAND_LAB_FRAME_LEN,
                &thread_commands,
            );
            info!(
                path = %path_log,
                records = stats.records,
                commands = stats.commands,
                "Command Lab USBPcap capture saved"
            );
        }) {
        Ok(thread) => thread,
        Err(error) => {
            warn!(%error, "Failed to spawn Command Lab capture thread");
            unsafe { CloseHandle(filter) };
            return Err(CaptureError::Unavailable);
        }
    };

    Ok(CommandLabCapture {
        inner: CaptureInner::Native {
            cancel,
            read_thread: Some(read_thread),
            commands,
        },
    })
}

/// Requests administrator privileges and launches the elevated capture
/// process. Blocks while the UAC prompt is shown; returns `None` when the
/// request was declined or the child failed to start its capture.
fn start_elevated_capture(output_path: &Path) -> Option<CommandLabCapture> {
    let exe = std::env::current_exe().ok()?;
    let parameters = format!("--command-lab-capture \"{}\"", output_path.display());
    let process = spawn_elevated_process(&exe, &parameters)?;
    // An immediate exit means the child could not start its capture.
    if unsafe { WaitForSingleObject(process, 0) } == WAIT_OBJECT_0 {
        let mut exit_code = 0;
        unsafe { GetExitCodeProcess(process, &mut exit_code) };
        unsafe { CloseHandle(process) };
        warn!(
            exit_code,
            "Elevated Command Lab capture exited before starting"
        );
        return None;
    }
    Some(CommandLabCapture {
        inner: CaptureInner::Elevated {
            process,
            output_path: output_path.to_path_buf(),
            commands: Arc::new(AtomicU64::new(0)),
        },
    })
}

/// Launches `exe` elevated via ShellExecuteEx with the `runas` verb (UAC).
fn spawn_elevated_process(
    exe: &std::path::Path,
    parameters: &str,
) -> Option<windows_sys::Win32::Foundation::HANDLE> {
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NO_CONSOLE | SEE_MASK_FLAG_NO_UI;
    info.nShow = SW_HIDE;
    let verb: Vec<u16> = "runas".encode_utf16().chain(std::iter::once(0)).collect();
    let file: Vec<u16> = exe
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let params: Vec<u16> = parameters
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = params.as_ptr();
    let launched = unsafe { ShellExecuteExW(&mut info) };
    if launched == 0 {
        let error = unsafe { GetLastError() };
        warn!(
            %error,
            "Failed to request elevated Command Lab capture (user declined?)"
        );
        return None;
    }
    Some(info.hProcess)
}

/// Path of the count sidecar file an elevated capture updates.
fn count_sidecar_path(output_path: &std::path::Path) -> PathBuf {
    let mut file_name = output_path
        .file_name()
        .unwrap_or_default()
        .to_os_string();
    file_name.push(".count");
    output_path.with_file_name(file_name)
}

fn write_count_sidecar(output_path: &std::path::Path, count: u32) {
    let _ = std::fs::write(count_sidecar_path(output_path), count.to_string());
}

const ELEVATED_SIDECAR_INTERVAL: Duration = Duration::from_millis(250);

/// Capture output path: `command_lab_capture_<unix seconds>.pcap` next to the
/// running executable.
fn command_lab_capture_output_path() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some(exe_dir.join(format!("command_lab_capture_{seconds}.pcap")))
}

struct UsbpcapFilterTarget {
    device_name: String,
}

/// Finds the USBPcap filter whose root hub hosts the Razer EC device
/// (VID 0x1532). Access-denied errors mean elevation is required.
fn find_razer_usbpcap_filter() -> Result<UsbpcapFilterTarget, CaptureError> {
    for index in 1..=MAX_USBPCAP_INTERFACES {
        let Some(hub_symlink) = root_hub_symlink(index) else {
            continue;
        };
        match root_hub_hosts_razer_device(&hub_symlink) {
            Ok(true) => {
                return Ok(UsbpcapFilterTarget {
                    device_name: usbpcap_interface_name(index),
                });
            }
            Ok(false) => continue,
            Err(CaptureError::AccessDenied) => return Err(CaptureError::AccessDenied),
            Err(CaptureError::Unavailable) => continue,
        }
    }
    warn!("Command Lab capture unavailable: no USBPcap root hub hosts a Razer device (VID 0x{RAZER_VID:04X})");
    Err(CaptureError::Unavailable)
}

/// Opens `\\.\USBPcapN` with zero access and queries the root hub symlink.
fn root_hub_symlink(index: u32) -> Option<String> {
    let wide_name: Vec<u16> = usbpcap_interface_name(index)
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let handle =
        unsafe { CreateFileW(wide_name.as_ptr(), 0, 0, null_mut(), OPEN_EXISTING, 0, 0) };
    if handle == INVALID_HANDLE_VALUE {
        return None;
    }
    let mut symlink = vec![0u16; HUB_SYMLINK_BUFFER_WORDS];
    let mut bytes_returned = 0u32;
    let succeeded = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_USBPCAP_GET_HUB_SYMLINK,
            null(),
            0,
            symlink.as_mut_ptr().cast(),
            (HUB_SYMLINK_BUFFER_WORDS * std::mem::size_of::<u16>()) as u32,
            &mut bytes_returned,
            null_mut(),
        )
    };
    unsafe { CloseHandle(handle) };
    if succeeded == 0 || bytes_returned == 0 {
        return None;
    }
    let end = symlink.iter().position(|&c| c == 0).unwrap_or(symlink.len());
    Some(String::from_utf16_lossy(&symlink[..end]))
}

/// Scans the root hub ports for a Razer device. Access-denied errors mean
/// elevation is required.
fn root_hub_hosts_razer_device(hub_symlink: &str) -> Result<bool, CaptureError> {
    let wide_hub: Vec<u16> = hub_symlink
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let hub = unsafe {
        CreateFileW(
            wide_hub.as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_WRITE,
            null_mut(),
            OPEN_EXISTING,
            0,
            0,
        )
    };
    if hub == INVALID_HANDLE_VALUE {
        let error = unsafe { GetLastError() };
        if error == ERROR_ACCESS_DENIED {
            warn!("Command Lab capture requires administrator privileges");
            return Err(CaptureError::AccessDenied);
        }
        warn!(%error, hub = %hub_symlink, "Failed to open USB root hub");
        return Err(CaptureError::Unavailable);
    }

    let mut node_info = [0u8; 256];
    let mut bytes_returned = 0u32;
    let port_count = unsafe {
        DeviceIoControl(
            hub,
            IOCTL_USB_GET_NODE_INFORMATION,
            node_info.as_mut_ptr().cast(),
            node_info.len() as u32,
            node_info.as_mut_ptr().cast(),
            node_info.len() as u32,
            &mut bytes_returned,
            null_mut(),
        )
    };
    if port_count == 0 || (bytes_returned as usize) < NODE_INFORMATION_PORT_COUNT_OFFSET + 1 {
        unsafe { CloseHandle(hub) };
        warn!(hub = %hub_symlink, "Failed to query USB root hub ports");
        return Err(CaptureError::Unavailable);
    }
    let port_count = node_info[NODE_INFORMATION_PORT_COUNT_OFFSET];

    let mut razer = false;
    let mut connection = [0u8; 128];
    for port in 1..=port_count {
        connection[NODE_CONNECTION_INDEX_OFFSET..NODE_CONNECTION_INDEX_OFFSET + 4]
            .copy_from_slice(&(port as u32).to_le_bytes());
        let succeeded = unsafe {
            DeviceIoControl(
                hub,
                IOCTL_USB_GET_NODE_CONNECTION_INFORMATION,
                connection.as_mut_ptr().cast(),
                connection.len() as u32,
                connection.as_mut_ptr().cast(),
                connection.len() as u32,
                &mut bytes_returned,
                null_mut(),
            )
        };
        if succeeded == 0 {
            continue;
        }
        let status = u32::from_le_bytes(
            connection[NODE_CONNECTION_STATUS_OFFSET..NODE_CONNECTION_STATUS_OFFSET + 4]
                .try_into()
                .expect("status slice has fixed length"),
        );
        if status != USB_CONNECTION_DEVICE_CONNECTED {
            continue;
        }
        let id_vendor = u16::from_le_bytes(
            connection[NODE_CONNECTION_ID_VENDOR_OFFSET..NODE_CONNECTION_ID_VENDOR_OFFSET + 2]
                .try_into()
                .expect("vendor slice has fixed length"),
        );
        if id_vendor == RAZER_VID {
            razer = true;
            break;
        }
    }
    unsafe { CloseHandle(hub) };
    Ok(razer)
}

/// Opens the filter for capture and configures snaplen and the kernel
/// capture buffer. The driver captures every device on the root hub; the
/// frame-length filter is applied while reading.
fn open_and_configure_filter(
    target: &UsbpcapFilterTarget,
) -> Result<windows_sys::Win32::Foundation::HANDLE, CaptureError> {
    let wide_name: Vec<u16> = target
        .device_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe {
        CreateFileW(
            wide_name.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            null_mut(),
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED,
            0,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        let error = unsafe { GetLastError() };
        if error == ERROR_ACCESS_DENIED {
            warn!("Command Lab capture requires administrator privileges");
            return Err(CaptureError::AccessDenied);
        }
        warn!(%error, "Failed to open {}", target.device_name);
        return Err(CaptureError::Unavailable);
    }

    let mut bytes_returned = 0u32;
    let mut size = USBPCAP_SNAPLEN.to_le_bytes();
    if device_ioctl(
        handle,
        IOCTL_USBPCAP_SET_SNAPLEN_SIZE,
        &mut size,
        &mut bytes_returned,
    ) == 0
    {
        warn!("Failed to set USBPcap snap length; aborting capture");
        unsafe { CloseHandle(handle) };
        return Err(CaptureError::Unavailable);
    }
    let mut size = USBPCAP_BUFFER_LEN.to_le_bytes();
    if device_ioctl(
        handle,
        IOCTL_USBPCAP_SETUP_BUFFER,
        &mut size,
        &mut bytes_returned,
    ) == 0
    {
        warn!("Failed to set up USBPcap capture buffer; aborting capture");
        unsafe { CloseHandle(handle) };
        return Err(CaptureError::Unavailable);
    }
    let mut address_filter = usbpcap_capture_all_filter();
    if device_ioctl(
        handle,
        IOCTL_USBPCAP_START_FILTERING,
        &mut address_filter,
        &mut bytes_returned,
    ) == 0
    {
        warn!("Failed to start USBPcap filtering; aborting capture");
        unsafe { CloseHandle(handle) };
        return Err(CaptureError::Unavailable);
    }
    Ok(handle)
}

/// USBPCAP_ADDRESS_FILTER (packed) capturing from all devices.
fn usbpcap_capture_all_filter() -> [u8; USBPCAP_ADDRESS_FILTER_LEN] {
    let mut filter = [0u8; USBPCAP_ADDRESS_FILTER_LEN];
    filter[USBPCAP_ADDRESS_FILTER_LEN - 1] = 1;
    filter
}

fn device_ioctl(
    handle: windows_sys::Win32::Foundation::HANDLE,
    code: u32,
    buffer: &mut [u8],
    bytes_returned: &mut u32,
) -> i32 {
    unsafe {
        DeviceIoControl(
            handle,
            code,
            buffer.as_mut_ptr().cast(),
            buffer.len() as u32,
            buffer.as_mut_ptr().cast(),
            buffer.len() as u32,
            bytes_returned,
            null_mut(),
        )
    }
}

#[derive(Clone, Copy, Debug)]
struct CaptureStats {
    records: u64,
    commands: u64,
}

/// Reads USBPcap packets until `duration` elapses or `cancel` is set, writing
/// only frames matching `frame_len_filter` to the output file and counting
/// parsed commands in `commands`.
fn read_and_filter(
    filter: windows_sys::Win32::Foundation::HANDLE,
    mut output: std::fs::File,
    duration: Duration,
    cancel: &AtomicBool,
    frame_len_filter: u32,
    commands: &AtomicU64,
) -> CaptureStats {
    let read_event = unsafe { CreateEventW(null_mut(), 1, 0, null()) };
    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    overlapped.hEvent = read_event;
    let mut buffer = vec![0u8; USBPCAP_BUFFER_LEN as usize];
    let mut sink = CaptureSink::new(&mut output, commands);
    let deadline = Instant::now() + duration;

    if !start_read(filter, &mut buffer, &mut overlapped) {
        warn!("USBPcap read failed immediately; capture aborted");
        unsafe { CloseHandle(filter) };
        unsafe { CloseHandle(read_event) };
        return sink.stats();
    }

    while !cancel.load(Ordering::SeqCst) && Instant::now() < deadline {
        let wait =
            unsafe { WaitForSingleObject(read_event, READ_POLL_INTERVAL.as_millis() as u32) };
        if wait == WAIT_OBJECT_0 {
            let mut bytes_read = 0u32;
            let ok = unsafe { GetOverlappedResult(filter, &overlapped, &mut bytes_read, 0) };
            unsafe { ResetEvent(read_event) };
            if ok == 0 || bytes_read == 0 {
                warn!("USBPcap read failed during capture; stopping");
                break;
            }
            if let Err(error) = sink.feed(&buffer[..bytes_read as usize], frame_len_filter) {
                warn!(%error, "Failed to write Command Lab capture; stopping");
                break;
            }
            if !start_read(filter, &mut buffer, &mut overlapped) {
                warn!("USBPcap read failed while re-arming; stopping");
                break;
            }
        } else if wait != WAIT_TIMEOUT {
            warn!(wait, "USBPcap read wait failed; stopping capture");
            break;
        }
    }

    unsafe { CancelIo(filter) };
    unsafe { CloseHandle(filter) };
    unsafe { CloseHandle(read_event) };
    let stats = sink.stats();
    drop(sink);
    let _ = output.flush();
    stats
}

/// Starts an overlapped read on the filter. Returns whether a completion is
/// pending (either completed synchronously or with ERROR_IO_PENDING).
fn start_read(
    filter: windows_sys::Win32::Foundation::HANDLE,
    buffer: &mut [u8],
    overlapped: &mut OVERLAPPED,
) -> bool {
    let completed = unsafe {
        ReadFile(
            filter,
            buffer.as_mut_ptr().cast(),
            buffer.len() as u32,
            null_mut(),
            overlapped,
        ) != 0
    };
    completed || unsafe { GetLastError() } == ERROR_IO_PENDING
}

/// Streaming pcap filter: forwards the 24-byte global header, then keeps only
/// records whose frame length equals the configured filter, counting the
/// commands extracted from each kept frame. Handles any read granularity,
/// including partial headers and records.
struct CaptureSink<'a> {
    output: &'a mut dyn Write,
    commands: &'a AtomicU64,
    header_bytes: usize,
    pending: Vec<u8>,
    records: u64,
}

impl<'a> CaptureSink<'a> {
    fn new(output: &'a mut dyn Write, commands: &'a AtomicU64) -> Self {
        Self {
            output,
            commands,
            header_bytes: 0,
            pending: Vec::new(),
            records: 0,
        }
    }

    fn feed(&mut self, data: &[u8], frame_len_filter: u32) -> io::Result<()> {
        self.pending.extend_from_slice(data);
        if self.header_bytes < PCAP_GLOBAL_HEADER_LEN {
            let take = self
                .pending
                .len()
                .min(PCAP_GLOBAL_HEADER_LEN - self.header_bytes);
            self.output.write_all(&self.pending[..take])?;
            self.pending.drain(..take);
            self.header_bytes += take;
            if self.header_bytes < PCAP_GLOBAL_HEADER_LEN {
                return Ok(());
            }
        }
        while self.pending.len() >= PCAP_RECORD_HEADER_LEN {
            let incl_len = u32::from_le_bytes(
                self.pending[8..12]
                    .try_into()
                    .expect("record length slice has fixed length"),
            ) as usize;
            let record_len = PCAP_RECORD_HEADER_LEN + incl_len;
            if self.pending.len() < record_len {
                break;
            }
            self.records += 1;
            if frame_len(&self.pending[..record_len]).is_some_and(|len| len == frame_len_filter) {
                self.output.write_all(&self.pending[..record_len])?;
                if extract_command(&self.pending[PCAP_RECORD_HEADER_LEN..record_len]).is_some() {
                    self.commands.fetch_add(1, Ordering::SeqCst);
                }
            }
            self.pending.drain(..record_len);
        }
        Ok(())
    }

    fn stats(&self) -> CaptureStats {
        CaptureStats {
            records: self.records,
            commands: self.commands.load(Ordering::SeqCst),
        }
    }
}

/// Total frame length from the packed USBPCAP_BUFFER_PACKET_HEADER:
/// `headerLen + dataLength`, which Wireshark reports as `frame.len`.
fn frame_len(record: &[u8]) -> Option<u32> {
    if record.len() < PCAP_RECORD_HEADER_LEN + USBPCAP_HEADER_LEN_MIN {
        return None;
    }
    let packet = &record[PCAP_RECORD_HEADER_LEN..];
    let header_len = u16::from_le_bytes([packet[0], packet[1]]) as u32;
    let data_len =
        u32::from_le_bytes(packet[23..27].try_into().expect("data length has fixed length"));
    Some(header_len + data_len)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a real-shaped 126-byte record: 28-byte USBPcap control header,
    /// 8-byte setup, 90-byte data fragment (padded with zeros beyond args).
    fn command_record(prefix: [u8; 5], arg_count: u8, command: [u8; 2], args: &[u8]) -> Vec<u8> {
        const FRAME_LEN: usize = 126;
        const HEADER_LEN: u16 = USBPCAP_HEADER_LEN_MIN as u16 + 1;
        let fragment_start = HEADER_LEN as usize + USB_SETUP_PACKET_LEN;
        let mut record = vec![0u8; PCAP_RECORD_HEADER_LEN + FRAME_LEN];
        record[8..12].copy_from_slice(&(FRAME_LEN as u32).to_le_bytes());
        let frame = &mut record[PCAP_RECORD_HEADER_LEN..];
        frame[0..2].copy_from_slice(&HEADER_LEN.to_le_bytes());
        frame[23..27].copy_from_slice(&((FRAME_LEN - HEADER_LEN as usize) as u32).to_le_bytes());
        frame[fragment_start..fragment_start + 5].copy_from_slice(&prefix);
        frame[fragment_start + FRAGMENT_PREFIX_LEN] = arg_count;
        frame[fragment_start + FRAGMENT_COMMAND_OFFSET..fragment_start + FRAGMENT_COMMAND_OFFSET + 2]
            .copy_from_slice(&command);
        frame[fragment_start + FRAGMENT_ARGS_OFFSET..fragment_start + FRAGMENT_ARGS_OFFSET + args.len()]
            .copy_from_slice(args);
        record
    }

    #[test]
    fn frame_len_sums_header_and_data_lengths() {
        let record = command_record([0; 5], 0, [0, 0], &[]);
        assert_eq!(
            frame_len(&record),
            Some((record.len() - PCAP_RECORD_HEADER_LEN) as u32)
        );
        assert_eq!(frame_len(&record[..20]), None);
    }

    #[test]
    fn extract_command_reads_prefix_count_and_command() {
        let record = command_record([0x00, 0xC8, 0x00, 0x00, 0x00], 2, [0x00, 0x04], &[0xAA, 0xBB]);
        let command = extract_command(&record[PCAP_RECORD_HEADER_LEN..]).expect("frame parses");

        assert_eq!(command.command, 0x0004);
        assert_eq!(command.args, vec![0xAA, 0xBB]);
    }

    #[test]
    fn extract_command_ignores_trailing_zero_padding() {
        let record = command_record([0; 5], 4, [0x00, 0x08], &[0, 0, 0, 0]);
        let command = extract_command(&record[PCAP_RECORD_HEADER_LEN..]).expect("frame parses");

        assert_eq!(command.command, 0x0008);
        assert_eq!(command.args, vec![0, 0, 0, 0]);
    }

    #[test]
    fn extract_command_rejects_truncated_fragments() {
        let record = command_record([0; 5], 4, [0x00, 0x08], &[0, 0, 0, 0]);
        let truncated = &record[..PCAP_RECORD_HEADER_LEN + 36 + 11];
        assert_eq!(extract_command(&truncated[PCAP_RECORD_HEADER_LEN..]), None);
        assert_eq!(extract_command(&[]), None);
    }

    #[test]
    fn sink_forwards_header_and_keeps_only_command_frames() {
        let mut written = Vec::new();
        let commands = AtomicU64::new(0);
        let stats = {
            let mut sink = CaptureSink::new(&mut written, &commands);
            sink.feed(&[0u8; PCAP_GLOBAL_HEADER_LEN], 126).unwrap();
            let mut matched = command_record([0; 5], 0, [0x00, 0x04], &[]);
            // A well-formed 77-byte frame that must not match the filter.
            let mut other = vec![0u8; PCAP_RECORD_HEADER_LEN + 77];
            other[8..12].copy_from_slice(&77u32.to_le_bytes());
            other[16..18].copy_from_slice(&(USBPCAP_HEADER_LEN_MIN as u16 + 1).to_le_bytes());
            other[16 + 23..16 + 27].copy_from_slice(&(77u32 - 28).to_le_bytes());
            let mut chunk = Vec::new();
            chunk.append(&mut matched);
            chunk.append(&mut other);
            sink.feed(&chunk, 126).unwrap();
            sink.stats()
        };

        assert_eq!(
            written.len(),
            PCAP_GLOBAL_HEADER_LEN + PCAP_RECORD_HEADER_LEN + 126
        );
        assert_eq!(stats.records, 2);
        assert_eq!(stats.commands, 1);
        assert_eq!(commands.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn sink_handles_split_header_and_split_records() {
        let mut written = Vec::new();
        let commands = AtomicU64::new(0);
        let stats = {
            let mut sink = CaptureSink::new(&mut written, &commands);
            let matched = command_record([0; 5], 0, [0x00, 0x04], &[]);
            sink.feed(&[0u8; 10], 126).unwrap();
            sink.feed(&[0u8; PCAP_GLOBAL_HEADER_LEN - 10], 126).unwrap();
            sink.feed(&matched[..40], 126).unwrap();
            sink.feed(&matched[40..], 126).unwrap();
            sink.stats()
        };

        assert_eq!(
            written.len(),
            PCAP_GLOBAL_HEADER_LEN + PCAP_RECORD_HEADER_LEN + 126
        );
        assert_eq!(stats.records, 1);
        assert_eq!(stats.commands, 1);
    }

    #[test]
    fn capture_output_path_lives_next_to_the_executable() {
        let Some(path) = command_lab_capture_output_path() else {
            return;
        };
        let exe = std::env::current_exe().unwrap();
        assert_eq!(path.parent(), exe.parent());
        assert!(path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                name.starts_with("command_lab_capture_") && name.ends_with(".pcap")
            }));
    }

    #[test]
    fn count_sidecar_sits_next_to_the_capture_file() {
        let path = std::path::Path::new(r"C:\dir\command_lab_capture_1.pcap");
        assert_eq!(
            count_sidecar_path(path),
            PathBuf::from(r"C:\dir\command_lab_capture_1.pcap.count")
        );
    }
}
