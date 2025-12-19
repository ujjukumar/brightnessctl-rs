use crate::monitors::Monitor;
use windows::Win32::Devices::Display::{GetMonitorBrightness, SetMonitorBrightness, GetVCPFeatureAndVCPFeatureReply, SetVCPFeature};
use std::thread;
use std::time::Duration;

pub fn get_brightness(monitor: &Monitor) -> Option<u32> {
    unsafe {
        // 1. Try DDC/CI (External Monitors) - PRIORITY
        let mut current_vcp: u32 = 0;
        let mut max_vcp: u32 = 0;
        if GetVCPFeatureAndVCPFeatureReply(monitor.physical, 0x10, None, &mut current_vcp as *mut _, Some(&mut max_vcp)) != 0 {
             if max_vcp > 0 {
                return Some((current_vcp * 100) / max_vcp);
             }
        }

        // 2. Try Internal Display API (Laptop panels) - FALLBACK
        let mut min: u32 = 0;
        let mut current: u32 = 0;
        let mut max: u32 = 0;
        if GetMonitorBrightness(monitor.physical, &mut min, &mut current, &mut max) != 0 {
            if max > min {
                let range = max - min;
                let offset = current - min;
                return Some((offset * 100) / range);
            }
        }
    }
    None
}

pub fn set_brightness(monitor: &Monitor, value: u32) -> bool {
    let value = value.clamp(0, 100);

    unsafe {
        // 1. Try DDC/CI - PRIORITY with RETRY
        let mut current_vcp: u32 = 0;
        let mut max_vcp: u32 = 0;
        if GetVCPFeatureAndVCPFeatureReply(monitor.physical, 0x10, None, &mut current_vcp as *mut _, Some(&mut max_vcp)) != 0 {
             if max_vcp > 0 {
                 let target = (value * max_vcp) / 100;
                 for _ in 0..3 {
                     if SetVCPFeature(monitor.physical, 0x10, target) != 0 {
                         return true;
                     }
                     thread::sleep(Duration::from_millis(50));
                 }
             }
        }

        // 2. Try Internal Display API - FALLBACK
        let mut min: u32 = 0;
        let mut current: u32 = 0;
        let mut max: u32 = 0;
        if GetMonitorBrightness(monitor.physical, &mut min, &mut current, &mut max) != 0 && max > min {
             let target = min + (value * (max - min)) / 100;
             return SetMonitorBrightness(monitor.physical, target) != 0;
        }
    }
    false
}
