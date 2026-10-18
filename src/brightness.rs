use crate::state::Backend;
use crate::wmi;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Devices::Display::{
    DestroyPhysicalMonitors, GetMonitorBrightness, GetVCPFeatureAndVCPFeatureReply,
    SetMonitorBrightness, SetVCPFeature, PHYSICAL_MONITOR,
};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

// VCP Code for Brightness
const VCP_CODE_BRIGHTNESS: u8 = 0x10;

/// An owned Dxva2 physical monitor handle, destroyed on drop.
///
/// Handles start at 0, so whether a monitor has one is expressed by owning this
/// (or not), never by a null check.
#[derive(Debug)]
pub struct PhysicalHandle(HANDLE);

// Dxva2 handles aren't bound to the thread that opened them.
unsafe impl Send for PhysicalHandle {}

impl PhysicalHandle {
    pub fn new(handle: HANDLE) -> Self {
        Self(handle)
    }

    pub fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for PhysicalHandle {
    fn drop(&mut self) {
        let pm = PHYSICAL_MONITOR {
            hPhysicalMonitor: self.0,
            ..Default::default()
        };
        unsafe {
            let _ = DestroyPhysicalMonitors(&[pm]);
        }
    }
}

/// Hardware access for one monitor. Values are percentages (0-100).
pub trait BrightnessBackend {
    fn read(&mut self) -> Option<u32>;
    /// True if the monitor accepted the command.
    fn write(&mut self, percent: u32) -> bool;
    /// After a successful write: true if the monitor reports `percent` back.
    fn verify(&mut self, percent: u32) -> bool {
        self.read() == Some(percent)
    }
}

/// The raw brightness range a monitor reports. Everything above the hardware
/// boundary works in percent, so all conversions go through here.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Range {
    min: u32,
    max: u32,
}

impl Range {
    /// An empty or inverted range can't be scaled, so it falls back to 0-100.
    fn new(min: u32, max: u32) -> Self {
        if max > min { Self { min, max } } else { Self { min: 0, max: 100 } }
    }

    fn span(self) -> u64 {
        u64::from(self.max - self.min)
    }

    /// Percent to the nearest raw value.
    fn to_raw(self, percent: u32) -> u32 {
        let scaled = (u64::from(percent.min(100)) * self.span() + 50) / 100;
        self.min + scaled as u32
    }

    /// Raw value to the nearest percent.
    fn to_percent(self, raw: u32) -> u32 {
        let offset = u64::from(raw.clamp(self.min, self.max) - self.min);
        ((offset * 100 + self.span() / 2) / self.span()) as u32
    }
}

/// Dxva2's high-level API: (min, current, max), all raw.
fn read_high_level(handle: &PhysicalHandle) -> Option<(u32, u32, u32)> {
    let (mut min, mut current, mut max) = (0, 0, 0);
    let ok = unsafe { GetMonitorBrightness(handle.raw(), &mut min, &mut current, &mut max) != 0 };
    ok.then_some((min, current, max))
}

/// Raw VCP 0x10: (current, max).
fn read_vcp(handle: &PhysicalHandle) -> Option<(u32, u32)> {
    let (mut current, mut max) = (0, 0);
    let ok = unsafe {
        GetVCPFeatureAndVCPFeatureReply(handle.raw(), VCP_CODE_BRIGHTNESS, None, &mut current, Some(&mut max)) != 0
    };
    ok.then_some((current, max))
}

struct HighLevelBackend {
    handle: PhysicalHandle,
    range: Range,
}

impl BrightnessBackend for HighLevelBackend {
    fn read(&mut self) -> Option<u32> {
        read_high_level(&self.handle).map(|(_, raw, _)| self.range.to_percent(raw))
    }

    fn write(&mut self, percent: u32) -> bool {
        unsafe { SetMonitorBrightness(self.handle.raw(), self.range.to_raw(percent)) != 0 }
    }

    /// Compares raw values: with fewer than 100 steps, several percents share one.
    fn verify(&mut self, percent: u32) -> bool {
        read_high_level(&self.handle).map(|(_, raw, _)| raw) == Some(self.range.to_raw(percent))
    }
}

struct VcpBackend {
    handle: PhysicalHandle,
    range: Range,
}

impl BrightnessBackend for VcpBackend {
    fn read(&mut self) -> Option<u32> {
        read_vcp(&self.handle).map(|(raw, _)| self.range.to_percent(raw))
    }

    fn write(&mut self, percent: u32) -> bool {
        unsafe { SetVCPFeature(self.handle.raw(), VCP_CODE_BRIGHTNESS, self.range.to_raw(percent)) != 0 }
    }

    fn verify(&mut self, percent: u32) -> bool {
        read_vcp(&self.handle).map(|(raw, _)| raw) == Some(self.range.to_raw(percent))
    }
}

struct WmiBackend {
    panel: wmi::Panel,
}

impl BrightnessBackend for WmiBackend {
    fn read(&mut self) -> Option<u32> {
        self.panel.get_brightness()
    }

    fn write(&mut self, percent: u32) -> bool {
        self.panel.set_brightness(percent)
    }

    /// The driver snaps writes to the panel's supported levels, so an exact read-back
    /// can legitimately differ; an accepted write counts as confirmed.
    fn verify(&mut self, _percent: u32) -> bool {
        true
    }
}

/// What probing found out about a monitor.
#[derive(Debug, Clone, PartialEq)]
pub struct Probe {
    pub backend: Backend,
    pub hardware_min: u32,
    pub hardware_max: u32,
    /// Current brightness in percent; `None` if no backend could read it.
    pub value: Option<u32>,
}

impl Probe {
    fn unsupported() -> Self {
        Self { backend: Backend::Unsupported, hardware_min: 0, hardware_max: 100, value: None }
    }
}

/// Finds a working backend for a monitor on the current thread and runs `f` with it.
///
/// Tries Dxva2's high-level API, then raw VCP 0x10, then WMI; `f` gets `None` if nothing
/// can read the monitor. COM and WMI are only set up when Dxva2 fails, so desktops never
/// pay for them.
pub fn with_probed_backend(
    physical: Option<PhysicalHandle>,
    device_path: &str,
    f: impl FnOnce(Probe, Option<&mut dyn BrightnessBackend>),
) {
    if let Some(handle) = physical {
        if let Some((min, current, max)) = read_high_level(&handle) {
            let range = Range::new(min, max);
            let probe = Probe {
                backend: Backend::HighLevel,
                hardware_min: range.min,
                hardware_max: range.max,
                value: Some(range.to_percent(current)),
            };
            return f(probe, Some(&mut HighLevelBackend { handle, range }));
        }

        if let Some((current, max)) = read_vcp(&handle) {
            let range = Range::new(0, max);
            let probe = Probe {
                backend: Backend::Vcp { max: range.max },
                hardware_min: range.min,
                hardware_max: range.max,
                value: Some(range.to_percent(current)),
            };
            return f(probe, Some(&mut VcpBackend { handle, range }));
        }
    }

    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
    // Scoped so every COM object is released before COM is uninitialized.
    {
        let panel = wmi::Connection::connect().ok().and_then(|c| c.find_panel(device_path));
        match panel.and_then(|panel| panel.get_brightness().map(|value| (panel, value))) {
            Some((panel, value)) => {
                let probe = Probe { backend: Backend::Wmi, hardware_min: 0, hardware_max: 100, value: Some(value) };
                f(probe, Some(&mut WmiBackend { panel }));
            }
            None => f(Probe::unsupported(), None),
        }
    }
    if initialized {
        unsafe { CoUninitialize() };
    }
}

pub fn should_restore(saved: &crate::settings::MonitorState) -> bool {
    saved.last_set_by_app && saved.write_confirmed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::MonitorState;

    #[test]
    #[ignore = "requires connected monitors; reads brightness only"]
    fn test_probe_reads_every_monitor() {
        for mut m in crate::monitors::enumerate_monitors() {
            let started = std::time::Instant::now();
            with_probed_backend(m.physical.take(), &m.device_path, |probe, backend| {
                println!("{} -> {:?} in {:?}", m.device_path, probe, started.elapsed());
                assert!(probe.value.is_some(), "could not read {}", m.device_path);
                assert!(backend.is_some());
                assert_ne!(probe.backend, Backend::Unsupported);
            });
        }
    }

    #[test]
    fn test_range_endpoints() {
        for (min, max) in [(0, 100), (0, 255), (0, 50), (10, 60)] {
            let range = Range::new(min, max);
            assert_eq!(range.to_raw(0), min);
            assert_eq!(range.to_raw(100), max);
            assert_eq!(range.to_percent(min), 0);
            assert_eq!(range.to_percent(max), 100);
        }
    }

    #[test]
    fn test_range_percent_round_trip() {
        // With at least 100 raw steps every percent survives a write and read back.
        for max in [100, 255] {
            let range = Range::new(0, max);
            for percent in 0..=100 {
                assert_eq!(range.to_percent(range.to_raw(percent)), percent, "max {max}, {percent}%");
            }
        }
    }

    #[test]
    fn test_range_with_fewer_steps_than_percents() {
        let range = Range::new(0, 50);
        // Every raw level is reachable and reads back as the percent that sets it.
        for raw in 0..=50 {
            assert_eq!(range.to_raw(range.to_percent(raw)), raw);
        }
        // A percent reads back as the nearest one the monitor can show.
        for percent in 0..=100 {
            let back = range.to_percent(range.to_raw(percent));
            assert!(back.abs_diff(percent) <= 1, "{percent}% read back as {back}%");
        }
    }

    #[test]
    fn test_range_rounds_instead_of_truncating() {
        let range = Range::new(0, 255);
        assert_eq!(range.to_raw(50), 128); // 127.5
        assert_eq!(range.to_raw(1), 3); // 2.55
        assert_eq!(range.to_percent(254), 100); // 99.6
        assert_eq!(range.to_percent(1), 0); // 0.39
    }

    #[test]
    fn test_range_clamps_and_rejects_empty_ranges() {
        let range = Range::new(10, 60);
        assert_eq!(range.to_percent(0), 0);
        assert_eq!(range.to_percent(200), 100);
        assert_eq!(range.to_raw(150), 60);
        assert_eq!(range.to_raw(50), 35);

        assert_eq!(Range::new(0, 0), Range::new(0, 100));
        assert_eq!(Range::new(80, 20), Range::new(0, 100));
    }

    #[test]
    fn test_probe_without_handle_or_wmi_is_unsupported() {
        // No Dxva2 handle and no WMI panel at this path.
        with_probed_backend(None, r"DISPLAY\NOPE0000\0&0&0&UID0", |probe, backend| {
            assert_eq!(probe, Probe::unsupported());
            assert!(backend.is_none());
        });
    }

    #[test]
    fn test_should_restore() {
        let mut state = MonitorState {
            brightness_value: 50,
            normalized_value: 0.5,
            last_set_by_app: true,
            write_confirmed: true,
            timestamp: 0,
            last_seen_device_path: "".into(),
        };
        assert!(should_restore(&state));

        state.write_confirmed = false;
        assert!(!should_restore(&state));

        state.write_confirmed = true;
        state.last_set_by_app = false;
        assert!(!should_restore(&state));
    }
}
