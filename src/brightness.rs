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

struct HighLevelBackend {
    handle: PhysicalHandle,
}

impl BrightnessBackend for HighLevelBackend {
    fn read(&mut self) -> Option<u32> {
        let (mut min, mut current, mut max) = (0, 0, 0);
        let ok = unsafe { GetMonitorBrightness(self.handle.raw(), &mut min, &mut current, &mut max) != 0 };
        ok.then_some(current)
    }

    fn write(&mut self, percent: u32) -> bool {
        unsafe { SetMonitorBrightness(self.handle.raw(), percent) != 0 }
    }
}

struct VcpBackend {
    handle: PhysicalHandle,
    max: u32,
}

impl VcpBackend {
    fn to_raw(&self, percent: u32) -> u32 {
        (percent * self.max) / 100
    }

    /// Current raw value and the monitor's maximum.
    fn read_raw(&mut self) -> Option<(u32, u32)> {
        let (mut current, mut max) = (0, 0);
        let ok = unsafe {
            GetVCPFeatureAndVCPFeatureReply(self.handle.raw(), VCP_CODE_BRIGHTNESS, None, &mut current, Some(&mut max)) != 0
        };
        ok.then_some((current, max))
    }
}

impl BrightnessBackend for VcpBackend {
    fn read(&mut self) -> Option<u32> {
        self.read_raw().map(|(raw, _)| (raw * 100) / self.max)
    }

    fn write(&mut self, percent: u32) -> bool {
        let raw = self.to_raw(percent);
        unsafe { SetVCPFeature(self.handle.raw(), VCP_CODE_BRIGHTNESS, raw) != 0 }
    }

    fn verify(&mut self, percent: u32) -> bool {
        let expected = self.to_raw(percent);
        self.read_raw().map(|(raw, _)| raw) == Some(expected)
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
        let (mut min, mut current, mut max) = (0, 0, 0);
        if unsafe { GetMonitorBrightness(handle.raw(), &mut min, &mut current, &mut max) } != 0 {
            let probe = Probe { backend: Backend::HighLevel, hardware_min: min, hardware_max: max, value: Some(current) };
            return f(probe, Some(&mut HighLevelBackend { handle }));
        }

        let mut vcp = VcpBackend { handle, max: 100 };
        if let Some((current, max)) = vcp.read_raw() {
            vcp.max = if max > 0 { max } else { 100 };
            let probe = Probe {
                backend: Backend::Vcp { max: vcp.max },
                hardware_min: 0,
                hardware_max: vcp.max,
                value: Some((current * 100) / vcp.max),
            };
            return f(probe, Some(&mut vcp));
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
