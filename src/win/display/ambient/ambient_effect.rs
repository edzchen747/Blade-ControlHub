use crate::razer::device_handle::DeviceHandle;
use crate::win::display::topology::{primary_display_device_name, wide_slice_to_os_string};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
#[cfg(debug_assertions)]
use tracing::trace;
use tracing::warn;
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
    IDXGIFactory1, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource,
};
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

fn run_ambient_loop(device_handle: DeviceHandle, current_generation: u32) {
    let Ok(mut capture) = DxgiSparseCapture::new() else {
        warn!("Ambient effect disabled because DXGI screen capture could not be initialized");
        schedule_ambient_recovery(device_handle, current_generation);
        return;
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
                warn!(%error, "Ambient effect stopped because DXGI screen capture failed");
                schedule_ambient_recovery(device_handle, current_generation);
                break;
            }
        }

        sleep_until_next_frame(start);
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
