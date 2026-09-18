/// Drives the ambient detection pipeline — capture, colour reduction, smoothing
/// and the final LED transform — without touching the keyboard, so a benchmark
/// measures exactly what the effect thread costs while it runs.
///
/// Every setting is the production one: the same sample grid, the same frame
/// pacing and the same capture backend the effect thread uses.
pub struct AmbientDetector {
    capture: AmbientCapture,
    smoother: ColorSmoother,
}

/// What a single detection step produced and what it cost.
#[derive(Clone, Copy, Debug)]
pub struct DetectionStep {
    pub rgb: (u8, u8, u8),
    pub brightness: u8,
    pub duration: Duration,
}

impl AmbientDetector {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            capture: AmbientCapture::open()?,
            smoother: ColorSmoother::new(),
        })
    }

    /// One full detection step: exactly the work `run_ambient_loop` performs
    /// between two `set_keyboard_color` calls.
    pub fn detect(&mut self) -> Result<DetectionStep, String> {
        let start = Instant::now();

        let color = self.capture.sample()?;
        let smoothed_rgb = self.smoother.smooth(color.rgb);
        let (saturated_rgb, brightness) = separate_saturated_rgb_brightness(smoothed_rgb);
        let brightness = apply_gamma_u8(brightness, 0.5);

        Ok(DetectionStep {
            rgb: (saturated_rgb.r, saturated_rgb.g, saturated_rgb.b),
            brightness,
            duration: start.elapsed(),
        })
    }

    /// Sleeps using the production pacing, including its busy-system backoff.
    pub fn pace_after(start: Instant) {
        sleep_until_next_frame(start);
    }

    /// Lowers the calling thread the way the effect thread lowers itself.
    pub fn apply_production_thread_priority() {
        demote_current_thread_priority();
    }

    /// Pins the process MTA, as the effect thread does before using WinRT.
    pub fn pin_process_mta() {
        ensure_process_mta();
    }

    /// Nominal capture rate before any backoff.
    pub fn target_fps() -> f64 {
        FPS
    }

    /// Sampled pixels per frame: the grid the colour reducer consumes.
    pub fn sample_grid() -> (u32, u32) {
        (SAMPLE_WIDTH, SAMPLE_HEIGHT)
    }
}

// ── Stage isolation ─────────────────────────────────────────────────────────

/// How much of a capture pipeline to stand up.
///
/// Holding a Desktop Duplication session open is what makes desktop window
/// animations stutter — `IdleDuplication` reproduces it without ever acquiring
/// a frame, and `WgcSparse` is the production path that does not. These stages
/// exist so that finding can be re-verified rather than taken on trust.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureStage {
    /// Nothing at all: the control.
    Baseline,
    /// Create the D3D11 device on the primary display adapter, nothing more.
    Device,
    /// Also call DuplicateOutput, then never acquire a frame. Stutters.
    IdleDuplication,
    /// Also acquire and release each frame, without copying or reading back.
    AcquireRelease,
    /// The whole production pipeline, on whichever backend it selects.
    Full,
    /// The Windows.Graphics.Capture frame source on its own.
    WgcSparse,
}

impl CaptureStage {
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "baseline" => Ok(Self::Baseline),
            "device" => Ok(Self::Device),
            "idle-duplication" => Ok(Self::IdleDuplication),
            "acquire-release" => Ok(Self::AcquireRelease),
            "full" => Ok(Self::Full),
            "wgc-sparse" => Ok(Self::WgcSparse),
            other => Err(format!("unknown stage {other}")),
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Baseline => "no GPU objects created (control)",
            Self::Device => "D3D11 device only, no capture",
            Self::IdleDuplication => "duplication session held open, never acquired",
            Self::AcquireRelease => "acquire + release each frame, no copy or readback",
            Self::Full => "full production detection pipeline",
            Self::WgcSparse => "WindowsGraphicsCapture session + sampled readback",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Device => "device",
            Self::IdleDuplication => "idle-duplication",
            Self::AcquireRelease => "acquire-release",
            Self::Full => "full",
            Self::WgcSparse => "wgc-sparse",
        }
    }

    pub fn all() -> [CaptureStage; 6] {
        [
            Self::Baseline,
            Self::Device,
            Self::IdleDuplication,
            Self::AcquireRelease,
            Self::Full,
            Self::WgcSparse,
        ]
    }
}

/// Holds whichever GPU objects a stage calls for and performs that stage's
/// per-frame work.
pub struct StageProbe {
    stage: CaptureStage,
    _device: Option<ID3D11Device>,
    duplication: Option<IDXGIOutputDuplication>,
    detector: Option<AmbientDetector>,
    wgc: Option<WgcSparseCapture>,
}

impl StageProbe {
    pub fn new(stage: CaptureStage) -> Result<Self, String> {
        Self::new_on_adapter(stage, None)
    }

    /// `adapter` pins the capture to one adapter index instead of the one
    /// driving the primary display, to test whether the choice of GPU matters.
    pub fn new_on_adapter(stage: CaptureStage, adapter: Option<u32>) -> Result<Self, String> {
        let empty = Self {
            stage,
            _device: None,
            duplication: None,
            detector: None,
            wgc: None,
        };

        match stage {
            CaptureStage::Baseline => return Ok(empty),
            CaptureStage::Full => {
                return Ok(Self {
                    detector: Some(AmbientDetector::new()?),
                    ..empty
                });
            }
            CaptureStage::WgcSparse => {
                return Ok(Self {
                    wgc: Some(WgcSparseCapture::new()?),
                    ..empty
                });
            }
            _ => {}
        }

        let (adapter, output) = match adapter {
            Some(index) => find_output_on_adapter(index)?,
            None => find_primary_output()?,
        };
        let device = create_capture_device(&adapter)?;

        let duplication = if stage == CaptureStage::Device {
            None
        } else {
            Some(unsafe {
                output
                    .DuplicateOutput(&device)
                    .map_err(|err| format!("DuplicateOutput: {err}"))?
            })
        };

        Ok(Self {
            _device: Some(device),
            duplication,
            ..empty
        })
    }

    /// How many frames Windows delivered to the WGC session across the run.
    /// With MinUpdateInterval honoured this tracks the sample count; without
    /// it, it tracks the display refresh rate instead.
    pub fn frames_delivered(&self) -> u64 {
        self.wgc.as_ref().map_or(0, |capture| capture.frames_seen())
    }

    /// One frame of this stage's work. True if a frame was actually captured.
    pub fn step(&mut self) -> Result<bool, String> {
        match self.stage {
            CaptureStage::Baseline | CaptureStage::Device | CaptureStage::IdleDuplication => {
                Ok(false)
            }
            CaptureStage::AcquireRelease => self.acquire_and_release(),
            CaptureStage::Full => {
                self.detector
                    .as_mut()
                    .ok_or_else(|| "full stage has no detector".to_string())?
                    .detect()?;
                Ok(true)
            }
            CaptureStage::WgcSparse => {
                self.wgc
                    .as_mut()
                    .ok_or_else(|| "wgc-sparse stage has no capture".to_string())?
                    .sample()?;
                Ok(true)
            }
        }
    }

    fn acquire_and_release(&mut self) -> Result<bool, String> {
        let duplication = self
            .duplication
            .as_ref()
            .ok_or_else(|| "acquire stage has no duplication".to_string())?;

        let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource: Option<IDXGIResource> = None;
        let acquired = unsafe { duplication.AcquireNextFrame(0, &mut frame_info, &mut resource) };

        match acquired {
            Ok(()) => {}
            Err(error) if error.code() == DXGI_ERROR_WAIT_TIMEOUT => return Ok(false),
            Err(error) => return Err(format!("AcquireNextFrame: {error}")),
        }

        drop(resource);
        let _ = unsafe { duplication.ReleaseFrame() };
        Ok(true)
    }
}

// ── Display topology ────────────────────────────────────────────────────────

/// Reports every adapter and output DXGI exposes, and whether each output can
/// be duplicated. On a hybrid-graphics laptop more than one adapter can expose
/// the same display, and capturing the wrong one forces cross-adapter copies
/// that the compositor pays for.
pub fn describe_adapters() -> Result<Vec<AdapterReport>, String> {
    let primary = primary_display_device_name();
    let factory: IDXGIFactory1 =
        unsafe { CreateDXGIFactory1().map_err(|err| format!("CreateDXGIFactory1: {err}"))? };

    let mut reports = Vec::new();
    for adapter_index in 0.. {
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(adapter_index) }) else {
            break;
        };
        let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
            continue;
        };

        let mut outputs = Vec::new();
        for output_index in 0.. {
            let Ok(output) = (unsafe { adapter.EnumOutputs(output_index) }) else {
                break;
            };
            let Ok(output_desc) = (unsafe { output.GetDesc() }) else {
                continue;
            };
            let name = wide_slice_to_os_string(&output_desc.DeviceName);
            let is_primary = primary.as_ref().is_some_and(|p| *p == name);

            let duplicable = output
                .cast::<IDXGIOutput1>()
                .map_err(|err| format!("{err}"))
                .and_then(|output1| {
                    let device = create_capture_device(&adapter)?;
                    unsafe {
                        output1
                            .DuplicateOutput(&device)
                            .map_err(|err| format!("{err}"))
                    }
                })
                .map(|_| ());

            outputs.push(OutputReport {
                name: name.to_string_lossy().into_owned(),
                attached: output_desc.AttachedToDesktop.as_bool(),
                is_primary,
                duplicable,
            });
        }

        reports.push(AdapterReport {
            index: adapter_index,
            description: String::from_utf16_lossy(&desc.Description)
                .trim_end_matches('\0')
                .to_string(),
            vendor_id: desc.VendorId,
            dedicated_video_memory: desc.DedicatedVideoMemory,
            outputs,
        });
    }

    Ok(reports)
}

pub struct AdapterReport {
    pub index: u32,
    pub description: String,
    pub vendor_id: u32,
    pub dedicated_video_memory: usize,
    pub outputs: Vec<OutputReport>,
}

pub struct OutputReport {
    pub name: String,
    pub attached: bool,
    pub is_primary: bool,
    pub duplicable: Result<(), String>,
}
