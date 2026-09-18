use crate::razer::device_handle::DeviceHandle;
use crate::win::display::topology::{primary_display_device_name, wide_slice_to_os_string};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
#[cfg(debug_assertions)]
use tracing::trace;
use tracing::{info, warn};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_11_0,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Resource, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
    DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO, IDXGIAdapter1,
    IDXGIDevice, IDXGIFactory1, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource,
};
use windows::Win32::System::Com::CoIncrementMTAUsage;
use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_LOWEST,
};
use windows::Foundation::TimeSpan;
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::SizeInt32;
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTOPRIMARY, MonitorFromPoint};
use windows::Win32::Foundation::POINT;
use windows::core::Interface;

const FPS: f64 = 15.0;
const SAMPLE_WIDTH: u32 = 32;
const SAMPLE_HEIGHT: u32 = 18;
const LERP_FACTOR: f32 = 0.6;
const BIN_LEVELS: usize = 16;
const BIN_COUNT: usize = BIN_LEVELS * BIN_LEVELS * BIN_LEVELS;

const BLACK_MAX_CHANNEL: u8 = 16;
const BLACK_LUMA_CUTOFF: u8 = 45;
const BLACK_LUMA_MAX_CHANNEL: u8 = 80;
const BLACK_MAX_SATURATION: f32 = 0.28;
const DARK_CHROMA_VISIBILITY_FLOOR: f32 = 0.08;
const FINAL_SATURATION_BOOST: f32 = 1.5;
const RED_SATURATION_BOOST: f32 = 1.5; // Boost red colors to correct keyboard LED inaccuracy
const SMOOTHED_CHROMA_FLOOR: f32 = 0.55;
const SMOOTHED_CHROMA_PULL: f32 = 0.50;
const BLACK_FALLBACK: Rgb = Rgb { r: 2, g: 2, b: 2 };
const AMBIENT_RECOVERY_DELAY: Duration = Duration::from_secs(3);

// The ambient effect is purely decorative, so it always yields to the desktop.
const GPU_THREAD_PRIORITY: i32 = -7;
// A sample normally costs a couple of milliseconds. Anything near this share of
// the frame budget means the GPU or CPU is contended (desktop animations do
// exactly that), and we stand down for a few frames instead of competing.
const BUSY_WORK_RATIO: f64 = 0.35;
const BUSY_BACKOFF_FRAMES: f64 = 3.0;

/// The capture pool format. B8G8R8A8 matches what the desktop composes in, so
/// Windows hands frames over without a conversion.
const CAPTURE_PIXEL_FORMAT: DirectXPixelFormat = DirectXPixelFormat::B8G8R8A8UIntNormalized;
/// The fewest buffers that still allow one frame to be read while the next
/// arrives.
const POOL_BUFFER_COUNT: i32 = 2;
/// How long to wait for the capture session's very first frame before giving up.
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(2);
const FIRST_FRAME_POLL: Duration = Duration::from_millis(15);

pub static THREAD_GENERATION: AtomicU32 = AtomicU32::new(0);
static AMBIENT_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

pub struct AmbientEffect {}

impl AmbientEffect {
    pub fn start(device_handle: DeviceHandle) {
        Self::stop();
        let current_generation = THREAD_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

        match thread::Builder::new()
            .name("blade-ambient-effect".to_string())
            .spawn(move || run_ambient_loop(device_handle, current_generation))
        {
            Ok(handle) => {
                *ambient_thread() = Some(handle);
            }
            Err(error) => {
                warn!(%error, "Failed to start ambient effect thread");
            }
        }
    }

    pub fn stop() {
        THREAD_GENERATION.fetch_add(1, Ordering::SeqCst);
        join_ambient_thread();
    }
}
fn ambient_thread() -> MutexGuard<'static, Option<JoinHandle<()>>> {
    AMBIENT_THREAD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn join_ambient_thread() {
    let current_thread_id = thread::current().id();
    let Some(handle) = ambient_thread().take() else {
        return;
    };

    if handle.thread().id() == current_thread_id {
        warn!("Skipping join of current ambient effect thread during shutdown");
        return;
    }

    if handle.join().is_err() {
        warn!("Ambient effect thread panicked during shutdown");
    }
}

/// The frame source the effect samples.
///
/// Windows.Graphics.Capture is preferred: holding a Desktop Duplication session
/// open pushes DWM off its flip path and makes desktop window animations
/// stutter, whether or not frames are actually being acquired. Duplication
/// remains the fallback for Windows builds without WGC.
enum AmbientCapture {
    Wgc(WgcSparseCapture),
    Duplication(DxgiSparseCapture),
}

impl AmbientCapture {
    fn open() -> Result<Self, String> {
        match WgcSparseCapture::new() {
            Ok(capture) => {
                info!("Ambient effect capturing through Windows.Graphics.Capture");
                return Ok(Self::Wgc(capture));
            }
            Err(error) => {
                warn!(%error, "Windows.Graphics.Capture unavailable; falling back to duplication");
            }
        }

        let capture = DxgiSparseCapture::new()?;
        warn!("Ambient effect is using Desktop Duplication, which can stutter desktop animations");
        Ok(Self::Duplication(capture))
    }

    fn sample(&mut self) -> Result<AmbientColor, String> {
        match self {
            Self::Wgc(capture) => capture.sample(),
            Self::Duplication(capture) => capture.sample(),
        }
    }
}

fn run_ambient_loop(device_handle: DeviceHandle, current_generation: u32) {
    demote_current_thread_priority();

    ensure_process_mta();

    let mut capture = match AmbientCapture::open() {
        Ok(capture) => capture,
        Err(error) => {
            warn!(%error, "Ambient effect disabled because screen capture could not start");
            schedule_ambient_recovery(device_handle, current_generation);
            return;
        }
    };

    let mut smoother = ColorSmoother::new();

    while THREAD_GENERATION.load(Ordering::SeqCst) == current_generation {
        let start = Instant::now();

        match capture.sample() {
            Ok(color) => {
                let smoothed_rgb = smoother.smooth(color.rgb);
                print_color_preview(color, smoothed_rgb);
                let (saturated_rgb, brightness) = separate_saturated_rgb_brightness(smoothed_rgb);
                device_handle.set_keyboard_color(
                    saturated_rgb.r,
                    saturated_rgb.g,
                    saturated_rgb.b,
                    apply_gamma_u8(brightness, 0.5), //  apply gamma transform to make keyboard LEDs slightly brighter
                );
            }
            Err(error) => {
                warn!(%error, "Ambient effect stopped because screen capture failed");
                schedule_ambient_recovery(device_handle, current_generation);
                break;
            }
        }

        sleep_until_next_frame(start);
    }
}

/// Pins the process-wide multithreaded apartment for the lifetime of the
/// process.
///
/// `windows-rs` caches WinRT activation factories (the ones behind
/// `GraphicsCaptureSession`) in process-wide statics. Those cached pointers are
/// only valid while the MTA is alive, and the audio module tears its apartment
/// down with `CoUninitialize` when it is finished with an endpoint. If that
/// drops the last MTA reference, every cached factory is freed and the next WGC
/// call jumps through a dangling vtable.
///
/// `CoIncrementMTAUsage` keeps the MTA alive independently of which threads
/// happen to be in it, so the cookie is deliberately never released. Plain
/// `CoInitializeEx` on this thread would not be enough: it only holds the
/// apartment while this one thread lives, and the ambient thread is stopped and
/// restarted whenever the effect is toggled.
fn ensure_process_mta() {
    static MTA: OnceLock<()> = OnceLock::new();

    MTA.get_or_init(|| {
        match unsafe { CoIncrementMTAUsage() } {
            Ok(_cookie) => info!("Pinned process MTA for Windows.Graphics.Capture"),
            Err(error) => warn!(%error, "Could not pin the process MTA; capture may be unavailable"),
        }
    });
}

fn demote_current_thread_priority() {
    if let Err(error) =
        unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_LOWEST) }
    {
        warn!(%error, "Failed to lower ambient effect thread priority");
    }
}

fn schedule_ambient_recovery(device_handle: DeviceHandle, current_generation: u32) {
    match thread::Builder::new()
        .name("blade-ambient-recovery".to_string())
        .spawn(move || {
            thread::sleep(AMBIENT_RECOVERY_DELAY);
            if THREAD_GENERATION.load(Ordering::SeqCst) == current_generation {
                device_handle.display_layout_changed();
            }
        }) {
        Ok(_handle) => {}
        Err(error) => {
            warn!(%error, "Failed to schedule ambient recovery");
        }
    }
}
