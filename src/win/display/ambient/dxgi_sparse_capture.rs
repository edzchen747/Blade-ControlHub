struct DxgiSparseCapture {
    _device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    staging: ID3D11Texture2D,
    width: u32,
    height: u32,
    format: DXGI_FORMAT,
    last: Option<AmbientColor>,
    reducer: AmbientReducer,
}

impl DxgiSparseCapture {
    fn new() -> Result<Self, String> {
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

                if let Ok(capture) = Self::from_output(adapter.clone(), output1) {
                    return Ok(capture);
                }
            }
        }

        Err("no duplicatable primary desktop output found".to_string())
    }

    fn from_output(adapter: IDXGIAdapter1, output: IDXGIOutput1) -> Result<Self, String> {
        let feature_levels = [D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_0];
        let mut device = None;
        let mut context = None;

        unsafe {
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&feature_levels),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .map_err(|err| format!("D3D11CreateDevice: {err}"))?;
        }

        let device = device.ok_or_else(|| "D3D11CreateDevice returned no device".to_string())?;
        let context = context.ok_or_else(|| "D3D11CreateDevice returned no context".to_string())?;
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

        let staging_desc = D3D11_TEXTURE2D_DESC {
            Width: SAMPLE_WIDTH,
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
        let mut staging = None;
        unsafe {
            device
                .CreateTexture2D(&staging_desc, None, Some(&mut staging))
                .map_err(|err| format!("CreateTexture2D sparse staging: {err}"))?;
        }
        let staging =
            staging.ok_or_else(|| "CreateTexture2D returned no staging texture".to_string())?;

        Ok(Self {
            _device: device,
            context,
            duplication,
            staging,
            width,
            height,
            format,
            last: None,
            reducer: AmbientReducer::new(),
        })
    }

    fn sample(&mut self) -> Result<AmbientColor, String> {
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
            Ok(()) => {}
            Err(error) if error.code() == DXGI_ERROR_WAIT_TIMEOUT => return Ok(None),
            Err(error) => return Err(format!("AcquireNextFrame: {error}")),
        }

        let result = (|| {
            let resource =
                resource.ok_or_else(|| "AcquireNextFrame returned no resource".to_string())?;
            let source = resource
                .cast::<ID3D11Texture2D>()
                .map_err(|err| format!("desktop resource cast: {err}"))?;

            self.copy_sample_grid(&source);
            let color = self.reduce_staging()?;
            Ok(color)
        })();

        let _ = unsafe { self.duplication.ReleaseFrame() };
        result.map(Some)
    }

    fn copy_sample_grid(&self, source: &ID3D11Texture2D) {
        for row in 0..SAMPLE_HEIGHT {
            let y =
                (((row * self.height) + (self.height / 2)) / SAMPLE_HEIGHT).min(self.height - 1);
            for col in 0..SAMPLE_WIDTH {
                let x =
                    (((col * self.width) + (self.width / 2)) / SAMPLE_WIDTH).min(self.width - 1);
                let source_box = D3D11_BOX {
                    left: x,
                    top: y,
                    front: 0,
                    right: x + 1,
                    bottom: y + 1,
                    back: 1,
                };
                unsafe {
                    self.context.CopySubresourceRegion(
                        &self.staging,
                        0,
                        col,
                        row,
                        0,
                        source,
                        0,
                        Some(&source_box),
                    );
                }
            }
        }
    }

    fn reduce_staging(&mut self) -> Result<AmbientColor, String> {
        self.reducer.clear();

        let staging_resource: ID3D11Resource = self
            .staging
            .cast()
            .map_err(|err| format!("staging resource cast: {err}"))?;
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            self.context
                .Map(&staging_resource, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .map_err(|err| format!("Map sparse staging texture: {err}"))?;
        }

        for row in 0..SAMPLE_HEIGHT {
            for col in 0..SAMPLE_WIDTH {
                let offset = row as usize * mapped.RowPitch as usize + col as usize * 4;
                let base = mapped.pData as *const u8;
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

// --- Color Engine ---

