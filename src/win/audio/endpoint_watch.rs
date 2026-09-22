//! Notification-based tracking of the default audio endpoint.
//!
//! Anything holding a long-lived stream on an endpoint needs to know when the
//! user changes their default device. There is no error to watch for: the old
//! endpoint stays perfectly valid and simply stops carrying audio, so a capture
//! bound to it keeps succeeding and returns nothing, which is indistinguishable
//! from a silent system.
//!
//! Windows will tell us instead, through `IMMNotificationClient`. The
//! alternative is re-reading the default endpoint's id on a timer, which works
//! but reacts a beat late and spends a COM call per tick forever.

use super::AudioType;
use tracing::warn;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::Media::Audio::{
    DEVICE_STATE, EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient,
    IMMNotificationClient_Impl, MMDeviceEnumerator,
};
use windows::Win32::Devices::FunctionDiscovery::{
    PKEY_Device_EnumeratorName, PKEY_Device_FriendlyName,
};
use windows::Win32::Media::Audio::{AUDCLNT_SHAREMODE_SHARED, IAudioClient, IMMDevice};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, STGM_READ};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;
use windows::core::{PCWSTR, Result, implement};

/// The COM object Windows calls back into.
///
/// It holds nothing but a flag, deliberately. Callbacks arrive on an arbitrary
/// RPC thread, so the less that happens inside one the better: reopening a
/// capture from here would do real work on a thread that belongs to the audio
/// service, while it waits.
#[implement(IMMNotificationClient)]
struct DefaultEndpointCallback {
    flow: EDataFlow,
    role: ERole,
    changed: Arc<AtomicBool>,
}

// Implemented on the generated wrapper rather than on the struct: that is the
// identity type the vtable is built against. It derefs to the struct, so the
// fields below are reached exactly as if this were the struct itself.
impl IMMNotificationClient_Impl for DefaultEndpointCallback_Impl {
    fn OnDefaultDeviceChanged(
        &self,
        flow: EDataFlow,
        role: ERole,
        _default_device_id: &PCWSTR,
    ) -> Result<()> {
        // Windows reports every flow and role. Only the pair this watcher was
        // built for counts - a change of default *recording* device, or of the
        // communications role, must not disturb a playback capture.
        if flow == self.flow && role == self.role {
            self.changed.store(true, Ordering::SeqCst);
        }
        Ok(())
    }

    fn OnDeviceStateChanged(&self, _device_id: &PCWSTR, _state: DEVICE_STATE) -> Result<()> {
        Ok(())
    }

    fn OnDeviceAdded(&self, _device_id: &PCWSTR) -> Result<()> {
        Ok(())
    }

    fn OnDeviceRemoved(&self, _device_id: &PCWSTR) -> Result<()> {
        Ok(())
    }

    fn OnPropertyValueChanged(&self, _device_id: &PCWSTR, _key: &PROPERTYKEY) -> Result<()> {
        Ok(())
    }
}

/// A live subscription to default-device changes for one `AudioType`.
///
/// The enumerator is held for the watcher's whole life: the registration lives
/// on that object, so dropping it early would quietly cancel the subscription.
pub(crate) struct DefaultEndpointWatcher {
    enumerator: IMMDeviceEnumerator,
    callback: IMMNotificationClient,
    changed: Arc<AtomicBool>,
}

impl DefaultEndpointWatcher {
    pub(crate) fn new(io: AudioType) -> Result<Self> {
        let selection = io.endpoint_selection();
        let changed = Arc::new(AtomicBool::new(false));

        let callback: IMMNotificationClient = DefaultEndpointCallback {
            flow: selection.direction,
            role: selection.role,
            changed: Arc::clone(&changed),
        }
        .into();

        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            enumerator.RegisterEndpointNotificationCallback(&callback)?;

            Ok(Self {
                enumerator,
                callback,
                changed,
            })
        }
    }

    /// Whether the default has moved since this was last asked, clearing the
    /// flag as it reports.
    ///
    /// Consuming the flag rather than merely reading it means a change that
    /// arrives while the caller is busy is still seen exactly once.
    pub(crate) fn take_change(&self) -> bool {
        self.changed.swap(false, Ordering::SeqCst)
    }
}

impl Drop for DefaultEndpointWatcher {
    fn drop(&mut self) {
        // Windows holds a reference to the callback until this is called, and
        // would otherwise keep calling into it.
        unsafe {
            let _ = self
                .enumerator
                .UnregisterEndpointNotificationCallback(&self.callback);
        }
    }
}

/// What the system can tell us about an endpoint, for latency compensation.
///
/// Bluetooth playback arrives at the ear well after it passes through the
/// render stream we capture, so a visualiser driven by loopback runs ahead of
/// what the listener hears. Compensating means knowing how far ahead, and this
/// is what Windows actually exposes to ask with.
pub(crate) struct EndpointInfo {
    pub(crate) name: String,
    /// The bus the device sits on. Bluetooth audio enumerates as `BTHENUM`.
    pub(crate) bus: String,
    /// `IAudioClient::GetStreamLatency`, in milliseconds.
    ///
    /// `None` when the endpoint would not answer. Distinguished from `Some(0.0)`
    /// on purpose: a failed call reported as zero is indistinguishable from a
    /// device claiming no latency, and reads as a real measurement when it is
    /// the absence of one.
    pub(crate) stream_latency_ms: Option<f64>,
    /// The engine period, which is where shared-mode latency actually lives -
    /// several drivers report a flat zero for stream latency and put the real
    /// figure here.
    pub(crate) device_period_ms: Option<f64>,
}

impl EndpointInfo {
    pub(crate) fn is_bluetooth(&self) -> bool {
        let bus = self.bus.to_ascii_uppercase();
        bus.contains("BTH") || bus.contains("BLUETOOTH")
    }
}

/// Reads what the current default endpoint reports about itself.
pub(crate) fn endpoint_info(io: AudioType) -> Option<EndpointInfo> {
    let device = super::default_endpoint(io).ok()?;

    unsafe {
        let store = device.OpenPropertyStore(STGM_READ).ok()?;
        let name = property_string(&store, &PKEY_Device_FriendlyName);
        let bus = property_string(&store, &PKEY_Device_EnumeratorName);

        let (stream_latency_ms, device_period_ms) = endpoint_timings(&device);

        Some(EndpointInfo {
            name,
            bus,
            stream_latency_ms,
            device_period_ms,
        })
    }
}

/// What the endpoint will say about its own timing, in milliseconds.
///
/// The client has to be initialised before either call will answer - an
/// activated but uninitialised one reports nothing, which reads as a convincing
/// zero. The stream is never started, so this costs a format negotiation and no
/// audio.
unsafe fn endpoint_timings(device: &IMMDevice) -> (Option<f64>, Option<f64>) {
    unsafe {
        let Ok(client) = device.Activate::<IAudioClient>(CLSCTX_ALL, None) else {
            return (None, None);
        };
        let Ok(format) = client.GetMixFormat() else {
            return (None, None);
        };
        if format.is_null() {
            return (None, None);
        }

        let initialised = client.Initialize(AUDCLNT_SHAREMODE_SHARED, 0, 0, 0, format, None);
        CoTaskMemFree(Some(format as *const core::ffi::c_void));
        if initialised.is_err() {
            warn!(error = ?initialised, "Endpoint would not initialise for a timing query");
            return (None, None);
        }

        // Both in 100 ns units.
        let latency = client.GetStreamLatency().ok().map(|t| t as f64 / 10_000.0);

        let mut default_period = 0i64;
        let mut minimum_period = 0i64;
        let period = client
            .GetDevicePeriod(Some(&mut default_period), Some(&mut minimum_period))
            .ok()
            .map(|()| default_period as f64 / 10_000.0);

        (latency, period)
    }
}

/// One property as text, whatever variant type it is stored as.
unsafe fn property_string(store: &IPropertyStore, key: &PROPERTYKEY) -> String {
    unsafe {
        let Ok(value) = store.GetValue(key) else {
            return String::new();
        };
        PropVariantToStringAlloc(&value).map_or_else(
            |_| String::new(),
            |text| super::take_com_string(text),
        )
    }
}
