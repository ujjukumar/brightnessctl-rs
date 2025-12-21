use crate::state::Monitor;
use windows::Win32::Devices::Display::{
    GetMonitorBrightness, SetMonitorBrightness,
    GetVCPFeatureAndVCPFeatureReply, SetVCPFeature,
};

// VCP Code for Brightness
const VCP_CODE_BRIGHTNESS: u8 = 0x10;

pub fn get_brightness(m: &Monitor) -> Option<u32> {
    unsafe {
        // Try Internal (High-level API)
        let mut min = 0;
        let mut current = 0;
        let mut max = 0;
        // windows crate maps BOOL return to Ok(()) if success? No, documentation varies. 
        // Error implies i32. 0 is FALSE, Non-zero is TRUE.
        if GetMonitorBrightness(m.physical, &mut min, &mut current, &mut max) != 0 {
            return Some(current);
        }

        // Try External (DDC/CI)
        let mut current_vcp = 0;
        let mut max_vcp = 0;
        if GetVCPFeatureAndVCPFeatureReply(m.physical, VCP_CODE_BRIGHTNESS, None, &mut current_vcp, Some(&mut max_vcp)) != 0 {
            if max_vcp > 0 {
                return Some((current_vcp * 100) / max_vcp);
            }
            return Some(current_vcp);
        }
    }
    None
}

pub fn set_brightness(m: &Monitor, value: u32) -> bool {
    let value = value.clamp(0, 100);
    unsafe {
        // Try Internal
        if SetMonitorBrightness(m.physical, value) != 0 {
            return true;
        }

        // Try External
        let mut current_vcp = 0;
        let mut max_vcp = 0;
        if GetVCPFeatureAndVCPFeatureReply(m.physical, VCP_CODE_BRIGHTNESS, None, &mut current_vcp, Some(&mut max_vcp)) != 0 {
             let new_val = (value * max_vcp) / 100;
             if SetVCPFeature(m.physical, VCP_CODE_BRIGHTNESS, new_val) != 0 {
                 return true;
             }
        }
    }
    false
}
