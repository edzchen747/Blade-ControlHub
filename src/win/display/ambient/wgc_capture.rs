// Windows.Graphics.Capture frame source for the ambient effect.
//
// This is the backend Chrome and Discord use for screen sharing. Unlike Desktop
// Duplication it captures through DWM's own composition pipeline, so holding a
// session open does not push the compositor off its flip path and desktop
// animations stay smooth.
//
// The effect only needs a SAMPLE_WIDTH x SAMPLE_HEIGHT grid of colours, never
// whole frames, so this asks Windows for as little as possible:
//
//   * MinUpdateInterval caps delivery at the effect's own frame rate, so WGC
//     stops producing frames at the panel's refresh rate. On a 240Hz display
//     that is the difference between ~240 and ~15 captures a second.
//   * Two pool buffers, the minimum that still lets one be read while the next
//     is written.
//   * The cursor is excluded: it is noise for a colour average and compositing
//     it costs work.
//   * Only SAMPLE_HEIGHT scanlines are copied off each frame, into an
//     alternating pair of staging textures so the CPU read never waits on the
//     GPU.

struct WgcSparseCapture {
    _device: ID3D11Device,
    context: ID3D11DeviceContext,
    winrt_device: IDirect3DDevice,
    frame_pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    /// Held so the captured item outlives the session.
    _item: GraphicsCaptureItem,
    staging: [ID3D11Texture2D; 2],
    write_index: usize,
    primed: bool,
    sample_x: Vec<u32>,
    sample_y: Vec<u32>,
    width: u32,
    height: u32,
    last: Option<AmbientColor>,
    reducer: AmbientReducer,
    acquired_frames: u64,
}

impl WgcSparseCapture {
    fn new() -> Result<Self, String> {
        if !GraphicsCaptureSession::IsSupported().unwrap_or(false) {
            return Err("Windows.Graphics.Capture is not supported here".to_string());
        }

        // The capture device must sit on the adapter driving the primary
        // display, exactly as the duplication path requires.
        let (adapter, _output) = find_primary_output()?;
        let device = create_capture_device(&adapter)?;
        let context = device_context(&device)?;
        let winrt_device = winrt_device_from_d3d11(&device)?;

        let item = primary_monitor_capture_item()?;
        let size = item
            .Size()
            .map_err(|err| format!("GraphicsCaptureItem::Size: {err}"))?;
        if size.Width <= 0 || size.Height <= 0 {
            return Err("capture item reported a zero-sized monitor".to_string());
        }

        let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &winrt_device,
            CAPTURE_PIXEL_FORMAT,
            POOL_BUFFER_COUNT,
            size,
        )
        .map_err(|err| format!("CreateFreeThreaded: {err}"))?;

        let session = frame_pool
            .CreateCaptureSession(&item)
            .map_err(|err| format!("CreateCaptureSession: {err}"))?;

        configure_session(&session);
        session
            .StartCapture()
            .map_err(|err| format!("StartCapture: {err}"))?;

        let width = size.Width as u32;
        let height = size.Height as u32;
        let staging = [
            create_sample_staging(&device, width)?,
            create_sample_staging(&device, width)?,
        ];

        Ok(Self {
            _device: device,
            context,
            winrt_device,
            frame_pool,
            session,
            _item: item,
            staging,
            write_index: 0,
            primed: false,
            sample_x: sample_columns(width),
            sample_y: sample_rows(height),
            width,
            height,
            last: None,
            reducer: AmbientReducer::new(),
            acquired_frames: 0,
        })
    }

    /// Frames Windows has actually delivered to this session.
    fn frames_seen(&self) -> u64 {
        self.acquired_frames
    }

    fn sample(&mut self) -> Result<AmbientColor, String> {
        if let Some(color) = self.capture_frame()? {
            self.last = Some(color);
            return Ok(color);
        }

        // A pool with nothing waiting means the desktop has not changed since
        // the last sample, so the previous colour is still correct.
        if let Some(color) = self.last {
            return Ok(color);
        }

        // Nothing has arrived yet. The session only starts producing after the
        // first composition pass, and MinUpdateInterval paces it at the effect's
        // own rate, so the very first frame can take a couple of intervals.
        self.wait_for_first_frame()
    }

    fn wait_for_first_frame(&mut self) -> Result<AmbientColor, String> {
        let deadline = Instant::now() + FIRST_FRAME_TIMEOUT;
        while Instant::now() < deadline {
            thread::sleep(FIRST_FRAME_POLL);
            if let Some(color) = self.capture_frame()? {
                self.last = Some(color);
                return Ok(color);
            }
        }

        Err("WGC produced no frame within the startup timeout".to_string())
    }

    fn capture_frame(&mut self) -> Result<Option<AmbientColor>, String> {
        let Some(frame) = self.take_newest_frame() else {
            return Ok(None);
        };

        let result = self.sample_frame(&frame);
        let _ = frame.Close();
        result.map(Some)
    }

    /// Drains the pool and keeps only the most recent frame, so a sample never
    /// reports a colour that is several frames stale.
    fn take_newest_frame(&mut self) -> Option<Direct3D11CaptureFrame> {
        let mut newest: Option<Direct3D11CaptureFrame> = None;
        while let Ok(frame) = self.frame_pool.TryGetNextFrame() {
            if let Some(previous) = newest.replace(frame) {
                let _ = previous.Close();
            }
            self.acquired_frames += 1;
        }
        newest
    }

    fn sample_frame(&mut self, frame: &Direct3D11CaptureFrame) -> Result<AmbientColor, String> {
        // A resolution change resizes the pool rather than tearing the session
        // down, which would drop frames and restart the capture.
        if let Ok(content) = frame.ContentSize() {
            let (width, height) = (content.Width.max(0) as u32, content.Height.max(0) as u32);
            if width != 0 && height != 0 && (width != self.width || height != self.height) {
                self.resize(content)?;
            }
        }

        let surface = frame
            .Surface()
            .map_err(|err| format!("Direct3D11CaptureFrame::Surface: {err}"))?;
        let access: IDirect3DDxgiInterfaceAccess = surface
            .cast()
            .map_err(|err| format!("IDirect3DDxgiInterfaceAccess cast: {err}"))?;
        let source: ID3D11Texture2D = unsafe {
            access
                .GetInterface()
                .map_err(|err| format!("frame surface interface: {err}"))?
        };

        self.copy_sample_rows(&source);
        self.take_pipelined_sample()
    }

    fn resize(&mut self, size: SizeInt32) -> Result<(), String> {
        self.frame_pool
            .Recreate(
                &self.winrt_device,
                CAPTURE_PIXEL_FORMAT,
                POOL_BUFFER_COUNT,
                size,
            )
            .map_err(|err| format!("Direct3D11CaptureFramePool::Recreate: {err}"))?;

        let width = size.Width as u32;
        let height = size.Height as u32;
        let device = &self._device;
        self.staging = [
            create_sample_staging(device, width)?,
            create_sample_staging(device, width)?,
        ];
        self.write_index = 0;
        self.primed = false;
        self.sample_x = sample_columns(width);
        self.sample_y = sample_rows(height);
        self.width = width;
        self.height = height;
        Ok(())
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

    /// Reads back the rows copied during the previous frame so the map never
    /// blocks on the GPU.
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
                // The pool is created as B8G8R8A8, so the order is fixed.
                let b = unsafe { *base.add(offset) };
                let g = unsafe { *base.add(offset + 1) };
                let r = unsafe { *base.add(offset + 2) };
                self.reducer.add(Rgb { r, g, b });
            }
        }

        unsafe {
            self.context.Unmap(&staging_resource, 0);
        }

        Ok(self.reducer.finish(SAMPLE_WIDTH * SAMPLE_HEIGHT))
    }
}

impl Drop for WgcSparseCapture {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.frame_pool.Close();
    }
}

/// Keeps WGC from doing work the effect would only throw away.
fn configure_session(session: &GraphicsCaptureSession) {
    // The cursor is noise in a colour average and costs compositing work.
    if let Err(error) = session.SetIsCursorCaptureEnabled(false) {
        warn!(%error, "Could not disable cursor capture for ambient effect");
    }

    // The yellow capture outline Windows 11 draws is both a visual intrusion
    // and extra compositor work. Not permitted on every build.
    if let Err(error) = session.SetIsBorderRequired(false) {
        warn!(%error, "Could not disable the capture border for ambient effect");
    }

    // The single biggest saving: without this WGC produces frames at the
    // panel's refresh rate even though the effect consumes 15 a second.
    // Windows 11 22H2 and newer only, hence the soft failure.
    let interval = TimeSpan {
        Duration: (10_000_000.0 / FPS) as i64, // 100ns units
    };
    if let Err(error) = session.SetMinUpdateInterval(interval) {
        warn!(
            %error,
            "Capture update interval unsupported; WGC will deliver frames at display rate"
        );
    }
}

fn create_sample_staging(device: &ID3D11Device, width: u32) -> Result<ID3D11Texture2D, String> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: SAMPLE_HEIGHT,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
    };
    create_staging_texture(device, &desc)
}

fn sample_columns(width: u32) -> Vec<u32> {
    (0..SAMPLE_WIDTH)
        .map(|col| (((col * width) + (width / 2)) / SAMPLE_WIDTH).min(width.saturating_sub(1)))
        .collect()
}

fn sample_rows(height: u32) -> Vec<u32> {
    (0..SAMPLE_HEIGHT)
        .map(|row| (((row * height) + (height / 2)) / SAMPLE_HEIGHT).min(height.saturating_sub(1)))
        .collect()
}

fn primary_monitor_capture_item() -> Result<GraphicsCaptureItem, String> {
    let monitor = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
    if monitor.is_invalid() {
        return Err("could not resolve the primary monitor".to_string());
    }

    let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
        .map_err(|err| format!("IGraphicsCaptureItemInterop: {err}"))?;

    unsafe {
        interop
            .CreateForMonitor(monitor)
            .map_err(|err| format!("CreateForMonitor: {err}"))
    }
}

fn winrt_device_from_d3d11(device: &ID3D11Device) -> Result<IDirect3DDevice, String> {
    let dxgi_device: IDXGIDevice = device
        .cast()
        .map_err(|err| format!("IDXGIDevice cast: {err}"))?;

    let inspectable = unsafe {
        CreateDirect3D11DeviceFromDXGIDevice(&dxgi_device)
            .map_err(|err| format!("CreateDirect3D11DeviceFromDXGIDevice: {err}"))?
    };

    inspectable
        .cast()
        .map_err(|err| format!("IDirect3DDevice cast: {err}"))
}
