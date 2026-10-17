use crate::state::{Backend, Monitor};
use crate::wmi;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Devices::Display::{
    GetMonitorBrightness, SetMonitorBrightness,
    GetVCPFeatureAndVCPFeatureReply, SetVCPFeature,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// VCP Code for Brightness
const VCP_CODE_BRIGHTNESS: u8 = 0x10;

fn get_timestamp() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
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
            let panel = connection.find_panel(&m.device_path)?;
            let value = panel.get_brightness()?;
            m.hardware_min = 0;
            m.hardware_max = 100;
            m.backend = Backend::Wmi(panel);
            Some(value)
        })
        .collect()
}

fn probe_dxva2(m: &mut Monitor) -> Option<u32> {
    let handle = m.physical?;
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

pub fn set_brightness(m: &mut Monitor, value: u32) -> bool {
    set_brightness_internal(m, value, false)
}

pub fn set_brightness_forced(m: &mut Monitor, value: u32) -> bool {
    set_brightness_internal(m, value, true)
}

fn set_brightness_internal(m: &mut Monitor, value: u32, force: bool) -> bool {
    if m.is_disabled || matches!(m.backend, Backend::Unsupported) {
        return false;
    }

    let now = Instant::now();
    if !force {
        if let Some(last) = m.last_write_time {
            if now.duration_since(last) < Duration::from_millis(100) {
                return false;
            }
        }
    }

    let value = value.clamp(0, 100);
    let (success, confirmed) = match m.backend {
        Backend::Unsupported => return false,
        Backend::HighLevel => match m.physical {
            Some(handle) => {
                let ok = unsafe { SetMonitorBrightness(handle, value) != 0 };
                (ok, ok)
            }
            None => (false, false),
        },
        Backend::Vcp { max } => match m.physical {
            Some(handle) => write_vcp(m, handle, value, max),
            None => (false, false),
        },
        Backend::Wmi(ref panel) => {
            let ok = panel.set_brightness(value);
            (ok, ok)
        }
    };

    if success {
        m.last_write_time = Some(now);
        m.failure_count = 0;
        m.normalized_value = value as f32 / 100.0;
        m.last_set_by_app = true;
        m.timestamp = get_timestamp();
        m.write_confirmed = confirmed;
    } else {
        m.failure_count += 1;
        if m.failure_count >= 10 {
            m.is_disabled = true;
        }
    }

    success
}

/// Writes VCP 0x10 using the maximum cached at probe time, then reads it back.
/// Returns `(written, confirmed)`.
fn write_vcp(m: &mut Monitor, handle: HANDLE, value: u32, max: u32) -> (bool, bool) {
    let requested = (value * max) / 100;
    unsafe {
        if SetVCPFeature(handle, VCP_CODE_BRIGHTNESS, requested) == 0 {
            return (false, false);
        }
        let (mut readback, mut readback_max) = (0, 0);
        if GetVCPFeatureAndVCPFeatureReply(handle, VCP_CODE_BRIGHTNESS, None, &mut readback, Some(&mut readback_max)) == 0 {
            return (true, false);
        }
        if readback == requested {
            return (true, true);
        }
        if value == 100 && readback < requested {
            m.observed_max = readback;
        }
        if value == 0 && readback > requested {
            m.observed_min = readback;
        }
        (true, false)
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
    fn test_probe_all_reads_every_monitor() {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
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
    fn test_backoff_escalation() {
        let mut m = crate::state::Monitor {
            physical: None,
            hmonitor: 0,
            name: "Test".into(),
            identity: None,
            backend: Backend::Unsupported,
            normalized_value: 0.0,
            last_set_by_app: false,
            write_confirmed: false,
            timestamp: 0,
            device_path: "".into(),
            last_write_time: None,
            failure_count: 0,
            is_disabled: false,
            hardware_min: 0,
            hardware_max: 100,
            observed_min: 0,
            observed_max: 100,
        };

        for _ in 0..9 {
            m.failure_count += 1;
            if m.failure_count >= 10 { m.is_disabled = true; }
        }
        assert!(!m.is_disabled);
        
        m.failure_count += 1;
        if m.failure_count >= 10 { m.is_disabled = true; }
        assert!(m.is_disabled);
    }
}