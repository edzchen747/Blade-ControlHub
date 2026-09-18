struct DxgiSparseCapture {
    _device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    staging: [ID3D11Texture2D; 2],
    write_index: usize,
    primed: bool,
    sample_x: Vec<u32>,
    sample_y: Vec<u32>,
    width: u32,
    format: DXGI_FORMAT,
    last: Option<AmbientColor>,
    reducer: AmbientReducer,
    acquired_frames: u64,
}
impl DxgiSparseCapture {
    fn new() -> Result<Self, String> {
        let (adapter, output) = find_primary_output()?;
        Self::from_output(adapter, output)
    }

    fn from_output(adapter: IDXGIAdapter1, output: IDXGIOutput1) -> Result<Self, String> {
        let device = create_capture_device(&adapter)?;
        let context = device_context(&device)?;
        let duplication = unsafe {
            output
                .DuplicateOutput(&device)
                .map_err(|err| format!("DuplicateOutput: {err}"))?
        };
        let desc = unsafe { duplication.GetDesc() };
        let width = desc.ModeDesc.Width;
        let height = desc.ModeDesc.Height;
        let format = desc.ModeDesc.Format;
        if width == 0 || height == 0 {
            return Err("duplicated output has zero dimensions".to_string());
        }

        // One staging row per sampled scanline. The grid columns are carved out on
        // the CPU afterwards, so the GPU only sees SAMPLE_HEIGHT copies per frame
        // instead of one copy per sampled pixel.
        let staging_desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: SAMPLE_HEIGHT,
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let staging = [
            create_staging_texture(&device, &staging_desc)?,
            create_staging_texture(&device, &staging_desc)?,
        ];

        let sample_x = (0..SAMPLE_WIDTH)
            .map(|col| (((col * width) + (width / 2)) / SAMPLE_WIDTH).min(width - 1))
            .collect();
        let sample_y = (0..SAMPLE_HEIGHT)
            .map(|row| (((row * height) + (height / 2)) / SAMPLE_HEIGHT).min(height - 1))
            .collect();

        Ok(Self {
            _device: device,
            context,
            duplication,
            staging,
            write_index: 0,
            primed: false,
            sample_x,
            sample_y,
            width,
            format,
            last: None,
            reducer: AmbientReducer::new(),
            acquired_frames: 0,
        })
    }

    fn sample(&mut self) -> Result<AmbientColor, String> {
        // Once a colour is established a timeout just means the desktop did not
        // change, so reuse it instead of spinning the duplication API for 80ms.
        if self.last.is_some() {
            if let Some(color) = self.capture_frame()? {
                self.last = Some(color);
            }
            return self
                .last
                .ok_or_else(|| "DXGI had no first frame ready yet".to_string());
        }

        for attempt in 0..5 {
            match self.capture_frame() {
                Ok(Some(color))
                    if self.last.is_none() && attempt < 4 && is_black_fallback(color) =>
                {
                    thread::sleep(Duration::from_millis(20));
                }
                Ok(Some(color)) => {
                    self.last = Some(color);
                    return Ok(color);
                }
                Ok(None) if attempt < 4 => thread::sleep(Duration::from_millis(20)),
                Ok(None) => {
                    return self
                        .last
                        .ok_or_else(|| "DXGI had no first frame ready yet".to_string());
                }
                Err(error) => return Err(error),
            }
        }

        self.last
            .ok_or_else(|| "DXGI had no first frame ready yet".to_string())
    }

    fn capture_frame(&mut self) -> Result<Option<AmbientColor>, String> {
        let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource: Option<IDXGIResource> = None;
        let timeout_ms = if self.last.is_some() { 0 } else { 100 };
        let acquired = unsafe {
            self.duplication
                .AcquireNextFrame(timeout_ms, &mut frame_info, &mut resource)
        };

        match acquired {
            Ok(()) => self.acquired_frames += 1,
            Err(error) if error.code() == DXGI_ERROR_WAIT_TIMEOUT => return Ok(None),
            Err(error) => return Err(format!("AcquireNextFrame: {error}")),
        }

        let result = (|| {
            let resource =
                resource.ok_or_else(|| "AcquireNextFrame returned no resource".to_string())?;
            let source = resource
                .cast::<ID3D11Texture2D>()
                .map_err(|err| format!("desktop resource cast: {err}"))?;

            self.copy_sample_rows(&source);
            self.take_pipelined_sample()
        })();

        let _ = unsafe { self.duplication.ReleaseFrame() };
        result.map(Some)
    }

    fn copy_sample_rows(&self, source: &ID3D11Texture2D) {
        let staging = &self.staging[self.write_index];
        for (row, &y) in self.sample_y.iter().enumerate() {
            let source_box = D3D11_BOX {
                left: 0,
                top: y,
                front: 0,
                right: self.width,
                bottom: y + 1,
                back: 1,
            };
            unsafe {
                self.context.CopySubresourceRegion(
                    staging,
                    0,
                    0,
                    row as u32,
                    0,
                    source,
                    0,
                    Some(&source_box),
                );
            }
        }
    }

    /// Reads back the rows copied during the *previous* frame so the map never
    /// blocks on the GPU, which would serialise the capture against the compositor.
    fn take_pipelined_sample(&mut self) -> Result<AmbientColor, String> {
        let read_index = if self.primed {
            self.write_index ^ 1
        } else {
            self.write_index
        };
        self.primed = true;
        self.write_index ^= 1;

        self.reduce_staging(read_index)
    }

    fn reduce_staging(&mut self, read_index: usize) -> Result<AmbientColor, String> {
        self.reducer.clear();

        let staging_resource: ID3D11Resource = self.staging[read_index]
            .cast()
            .map_err(|err| format!("staging resource cast: {err}"))?;
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            self.context
                .Map(&staging_resource, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .map_err(|err| format!("Map sparse staging texture: {err}"))?;
        }

        let base = mapped.pData as *const u8;
        for row in 0..SAMPLE_HEIGHT as usize {
            let row_offset = row * mapped.RowPitch as usize;
            for &x in &self.sample_x {
                let offset = row_offset + x as usize * 4;
                let c0 = unsafe { *base.add(offset) };
                let c1 = unsafe { *base.add(offset + 1) };
                let c2 = unsafe { *base.add(offset + 2) };
                let (r, g, b) = match self.format {
                    DXGI_FORMAT_R8G8B8A8_UNORM | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB => (c0, c1, c2),
                    DXGI_FORMAT_B8G8R8A8_UNORM | DXGI_FORMAT_B8G8R8A8_UNORM_SRGB => (c2, c1, c0),
                    _ => (c2, c1, c0),
                };
                self.reducer.add(Rgb { r, g, b });
            }
        }

        unsafe {
            self.context.Unmap(&staging_resource, 0);
        }

        Ok(self.reducer.finish(SAMPLE_WIDTH * SAMPLE_HEIGHT))
    }

}

fn create_staging_texture(
    device: &ID3D11Device,
    desc: &D3D11_TEXTURE2D_DESC,
) -> Result<ID3D11Texture2D, String> {
    let mut staging = None;
    unsafe {
        device
            .CreateTexture2D(desc, None, Some(&mut staging))
            .map_err(|err| format!("CreateTexture2D sparse staging: {err}"))?;
    }
    staging.ok_or_else(|| "CreateTexture2D returned no staging texture".to_string())
}

/// Ask the GPU scheduler to rank our capture work below everything else on the
/// desktop so compositor animations are never queued behind an ambient frame.
fn demote_gpu_thread_priority(device: &ID3D11Device) {
    let Ok(dxgi_device) = device.cast::<IDXGIDevice>() else {
        return;
    };
    if let Err(error) = unsafe { dxgi_device.SetGPUThreadPriority(GPU_THREAD_PRIORITY) } {
        warn!(%error, "Failed to lower ambient effect GPU thread priority");
    }
}

/// Finds the adapter and output driving the primary display. Ambient capture
/// must duplicate that output specifically: duplicating a different GPU output
/// is what the cross-GPU stutter fix exists to prevent.
fn find_primary_output() -> Result<(IDXGIAdapter1, IDXGIOutput1), String> {
    let primary_display = primary_display_device_name()
        .ok_or_else(|| "primary display device could not be resolved".to_string())?;
    let factory: IDXGIFactory1 =
        unsafe { CreateDXGIFactory1().map_err(|err| format!("CreateDXGIFactory1: {err}"))? };

    for adapter_index in 0.. {
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(adapter_index) }) else {
            break;
        };

        for output_index in 0.. {
            let Ok(output) = (unsafe { adapter.EnumOutputs(output_index) }) else {
                break;
            };
            let Ok(output1) = output.cast::<IDXGIOutput1>() else {
                continue;
            };
            let Ok(desc) = (unsafe { output.GetDesc() }) else {
                continue;
            };
            if !desc.AttachedToDesktop.as_bool() {
                continue;
            }
            if wide_slice_to_os_string(&desc.DeviceName) != primary_display {
                continue;
            }

            return Ok((adapter.clone(), output1));
        }
    }

    Err("no duplicatable primary desktop output found".to_string())
}

/// Creates the D3D11 device ambient capture runs on, already demoted in the
/// GPU scheduler.
fn create_capture_device(adapter: &IDXGIAdapter1) -> Result<ID3D11Device, String> {
    let feature_levels = [D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_0];
    let mut device = None;

    unsafe {
        D3D11CreateDevice(
            adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&feature_levels),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )
        .map_err(|err| format!("D3D11CreateDevice: {err}"))?;
    }

    let device = device.ok_or_else(|| "D3D11CreateDevice returned no device".to_string())?;
    demote_gpu_thread_priority(&device);
    Ok(device)
}

fn device_context(device: &ID3D11Device) -> Result<ID3D11DeviceContext, String> {
    unsafe {
        device
            .GetImmediateContext()
            .map_err(|err| format!("GetImmediateContext: {err}"))
    }
}

/// Finds the first attached output on a specific adapter index.
fn find_output_on_adapter(index: u32) -> Result<(IDXGIAdapter1, IDXGIOutput1), String> {
    let factory: IDXGIFactory1 =
        unsafe { CreateDXGIFactory1().map_err(|err| format!("CreateDXGIFactory1: {err}"))? };
    let adapter = unsafe {
        factory
            .EnumAdapters1(index)
            .map_err(|err| format!("no adapter at index {index}: {err}"))?
    };

    for output_index in 0.. {
        let Ok(output) = (unsafe { adapter.EnumOutputs(output_index) }) else {
            break;
        };
        let Ok(desc) = (unsafe { output.GetDesc() }) else {
            continue;
        };
        if !desc.AttachedToDesktop.as_bool() {
            continue;
        }
        if let Ok(output1) = output.cast::<IDXGIOutput1>() {
            return Ok((adapter.clone(), output1));
        }
    }

    Err(format!("adapter {index} has no attached duplicatable output"))
}
