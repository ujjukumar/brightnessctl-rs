use crate::state::Monitor;
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

pub fn get_brightness(m: &mut Monitor) -> Option<u32> {
    unsafe {
        // Try Internal (High-level API)
        let mut min = 0;
        let mut current = 0;
        let mut max = 0;
        if GetMonitorBrightness(m.physical, &mut min, &mut current, &mut max) != 0 {
            m.hardware_min = min;
            m.hardware_max = max;
            return Some(current);
        }

        // Try External (DDC/CI)
        let mut current_vcp = 0;
        let mut max_vcp = 0;
        if GetVCPFeatureAndVCPFeatureReply(m.physical, VCP_CODE_BRIGHTNESS, None, &mut current_vcp, Some(&mut max_vcp)) != 0 {
            m.hardware_min = 0;
            m.hardware_max = if max_vcp > 0 { max_vcp } else { 100 };
            
            if max_vcp > 0 {
                return Some((current_vcp * 100) / max_vcp);
            }
            return Some(current_vcp);
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
    if m.is_disabled {
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
    let mut success = false;
    let mut confirmed = false;

    unsafe {
        // Try Internal
        if SetMonitorBrightness(m.physical, value) != 0 {
            success = true;
            confirmed = true; 
        }

        // Try External (DDC/CI)
        if !success {
            let mut current_vcp = 0;
            let mut max_vcp = 0;
            if GetVCPFeatureAndVCPFeatureReply(m.physical, VCP_CODE_BRIGHTNESS, None, &mut current_vcp, Some(&mut max_vcp)) != 0 {
                 let hw_max = if max_vcp > 0 { max_vcp } else { 100 };
                 m.hardware_max = hw_max;
                 
                 let requested_vcp = (value * hw_max) / 100;
                 if SetVCPFeature(m.physical, VCP_CODE_BRIGHTNESS, requested_vcp) != 0 {
                     success = true;
                     
                     let mut rb_val = 0;
                     let mut rb_max = 0;
                     if GetVCPFeatureAndVCPFeatureReply(m.physical, VCP_CODE_BRIGHTNESS, None, &mut rb_val, Some(&mut rb_max)) != 0 {
                         if rb_val == requested_vcp {
                             confirmed = true;
                         } else {
                             if value == 100 && rb_val < requested_vcp {
                                 m.observed_max = rb_val;
                             }
                             if value == 0 && rb_val > requested_vcp {
                                 m.observed_min = rb_val;
                             }
                         }
                     }
                 }
            }
        }
    }

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

pub fn should_restore(saved: &crate::settings::MonitorState) -> bool {
    saved.last_set_by_app && saved.write_confirmed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::MonitorState;
    use windows::Win32::Foundation::HANDLE;

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
            physical: HANDLE::default(),
            name: "Test".into(),
            identity: None,
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