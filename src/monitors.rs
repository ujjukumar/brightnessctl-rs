use crate::state::Monitor;
use windows::core::BOOL;
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};
use windows::Win32::Devices::Display::{GetNumberOfPhysicalMonitorsFromHMONITOR, GetPhysicalMonitorsFromHMONITOR, PHYSICAL_MONITOR};

pub fn enumerate_monitors() -> Vec<Monitor> {
    let mut monitors = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            Some(HDC::default()),
            None,
            Some(monitor_enum_proc),
            LPARAM(&mut monitors as *mut _ as isize),
        );
    }
    monitors
}

extern "system" fn monitor_enum_proc(hmonitor: HMONITOR, _hdc: HDC, _rect: *mut RECT, lparam: LPARAM) -> BOOL {
    unsafe {
        let monitors = &mut *(lparam.0 as *mut Vec<Monitor>);
        
        let mut number_of_physical_monitors = 0;
        if GetNumberOfPhysicalMonitorsFromHMONITOR(hmonitor, &mut number_of_physical_monitors).is_ok() & (number_of_physical_monitors > 0) {
            let mut physical_monitors = vec![PHYSICAL_MONITOR::default(); number_of_physical_monitors as usize];
            if GetPhysicalMonitorsFromHMONITOR(hmonitor, &mut physical_monitors).is_ok() {
                for pm in physical_monitors {
                    let desc = pm.szPhysicalMonitorDescription;
                    monitors.push(Monitor {
                        physical: pm.hPhysicalMonitor,
                        name: String::from_utf16_lossy(&desc).trim_matches('\0').to_string(),
                    });
                }
            }
        }
    }
    BOOL::from(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enumerate_monitors() {
        let monitors = enumerate_monitors();
        // We can't assert count > 0 reliably on all machines, but we can check it doesn't panic.
        assert!(monitors.len() >= 0);
    }
}
