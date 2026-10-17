use crate::state::{Backend, Monitor};
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

    fn read_raw(&mut self) -> Option<u32> {
        let (mut current, mut max) = (0, 0);
        let ok = unsafe {
            GetVCPFeatureAndVCPFeatureReply(self.handle.raw(), VCP_CODE_BRIGHTNESS, None, &mut current, Some(&mut max)) != 0
        };
        ok.then_some(current)
    }
}

impl BrightnessBackend for VcpBackend {
    fn read(&mut self) -> Option<u32> {
        self.read_raw().map(|raw| (raw * 100) / self.max)
    }

    fn write(&mut self, percent: u32) -> bool {
        let raw = self.to_raw(percent);
        unsafe { SetVCPFeature(self.handle.raw(), VCP_CODE_BRIGHTNESS, raw) != 0 }
    }

    fn verify(&mut self, percent: u32) -> bool {
        let expected = self.to_raw(percent);
        self.read_raw() == Some(expected)
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

/// Stands in when a backend can't be opened, so every write is reported as failed.
struct UnavailableBackend;

impl BrightnessBackend for UnavailableBackend {
    fn read(&mut self) -> Option<u32> {
        None
    }

    fn write(&mut self, _percent: u32) -> bool {
        false
    }
}

/// What a worker needs to open a monitor's backend on its own thread.
#[derive(Debug)]
pub enum BackendSpec {
    HighLevel(PhysicalHandle),
    Vcp { handle: PhysicalHandle, max: u32 },
    /// COM objects can't move between threads, so the worker connects to WMI itself.
    Wmi { device_path: String },
}

/// Takes what a worker needs out of `m`. `None` for monitors without a usable backend.
pub fn worker_spec(m: &mut Monitor) -> Option<BackendSpec> {
    match m.backend {
        Backend::HighLevel => m.physical.take().map(BackendSpec::HighLevel),
        Backend::Vcp { max } => m.physical.take().map(|handle| BackendSpec::Vcp { handle, max }),
        Backend::Wmi => Some(BackendSpec::Wmi { device_path: m.device_path.clone() }),
        Backend::Unsupported => None,
    }
}

/// Opens the backend described by `spec` on the current thread and runs `f` with it.
pub fn with_backend(spec: BackendSpec, f: impl FnOnce(&mut dyn BrightnessBackend)) {
    match spec {
        BackendSpec::HighLevel(handle) => f(&mut HighLevelBackend { handle }),
        BackendSpec::Vcp { handle, max } => f(&mut VcpBackend { handle, max }),
        BackendSpec::Wmi { device_path } => {
            let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
            // Scoped so every COM object is released before COM is uninitialized.
            {
                let panel = wmi::Connection::connect().ok().and_then(|c| c.find_panel(&device_path));
                match panel {
                    Some(panel) => f(&mut WmiBackend { panel }),
                    None => f(&mut UnavailableBackend),
                }
            }
            if initialized {
                unsafe { CoUninitialize() };
            }
        }
    }
}

/// Chooses each monitor's backend and returns its current brightness (0-100).
///
/// Tries Dxva2's high-level API, then raw VCP 0x10, then WMI. WMI is only
/// connected when a monitor needs it, so desktops never pay for it.
pub fn probe_all(monitors: &mut [Monitor]) -> Vec<Option<u32>> {
    let mut wmi_connection: Option<Option<wmi::Connection>> = None;
    monitors
        .iter_mut()
        .map(|m| {
            if let Some(value) = probe_dxva2(m) {
                return Some(value);
            }
            let connection = wmi_connection
                .get_or_insert_with(|| wmi::Connection::connect().ok())
                .as_ref()?;
            let value = connection.find_panel(&m.device_path)?.get_brightness()?;
            m.hardware_min = 0;
            m.hardware_max = 100;
            m.backend = Backend::Wmi;
            Some(value)
        })
        .collect()
}

fn probe_dxva2(m: &mut Monitor) -> Option<u32> {
    let handle = m.physical.as_ref()?.raw();
    unsafe {
        let (mut min, mut current, mut max) = (0, 0, 0);
        if GetMonitorBrightness(handle, &mut min, &mut current, &mut max) != 0 {
            m.hardware_min = min;
            m.hardware_max = max;
            m.backend = Backend::HighLevel;
            return Some(current);
        }

        let (mut current, mut max) = (0, 0);
        if GetVCPFeatureAndVCPFeatureReply(handle, VCP_CODE_BRIGHTNESS, None, &mut current, Some(&mut max)) != 0 {
            let max = if max > 0 { max } else { 100 };
            m.hardware_min = 0;
            m.hardware_max = max;
            m.backend = Backend::Vcp { max };
            return Some((current * 100) / max);
        }
    }
    None
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
    fn test_probe_all_reads_every_monitor() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let mut monitors = crate::monitors::enumerate_monitors();
        let values = probe_all(&mut monitors);
        for (m, value) in monitors.iter().zip(&values) {
            println!("{} -> {:?} = {:?}", m.device_path, m.backend, value);
            assert!(value.is_some(), "could not read {}", m.device_path);
            assert!(!matches!(m.backend, Backend::Unsupported));
        }
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

    #[test]
    fn test_worker_spec_takes_the_handle() {
        let mut m = Monitor::new(None, 0, "Test".into(), None, r"DISPLAY\TEST\1".into());
        assert!(worker_spec(&mut m).is_none(), "unsupported monitors get no worker");

        m.backend = Backend::HighLevel;
        assert!(worker_spec(&mut m).is_none(), "no handle, no Dxva2 worker");

        m.backend = Backend::Wmi;
        match worker_spec(&mut m) {
            Some(BackendSpec::Wmi { device_path }) => assert_eq!(device_path, r"DISPLAY\TEST\1"),
            other => panic!("unexpected spec {other:?}"),
        }
    }
}
