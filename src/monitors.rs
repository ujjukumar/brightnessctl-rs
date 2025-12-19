use windows::Win32::Foundation::{BOOL, HANDLE, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};
use windows::Win32::Devices::Display::{GetNumberOfPhysicalMonitorsFromHMONITOR, GetPhysicalMonitorsFromHMONITOR, PHYSICAL_MONITOR, DestroyPhysicalMonitor};

pub struct Monitor {
    pub hmonitor: HMONITOR,
    pub physical: HANDLE,
    pub name: String,
}

impl Drop for Monitor {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyPhysicalMonitor(self.physical);
        }
    }
}

pub fn enumerate_monitors() -> Vec<Monitor> {
    let mut monitors = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(monitor_enum_proc),
            LPARAM(&mut monitors as *mut _ as isize),
        );
    }
    monitors
}

extern "system" fn monitor_enum_proc(hmonitor: HMONITOR, _hdc: HDC, _rect: *mut RECT, lparam: LPARAM) -> BOOL {
    unsafe {
        let monitors_ptr = lparam.0 as *mut Vec<Monitor>;
        let monitors = &mut *monitors_ptr;

        let mut count = 0;
        if GetNumberOfPhysicalMonitorsFromHMONITOR(hmonitor, &mut count).is_ok() && count > 0 {
            let mut physical_monitors = vec![PHYSICAL_MONITOR::default(); count as usize];
            if GetPhysicalMonitorsFromHMONITOR(hmonitor, &mut physical_monitors).is_ok() {
                for pm in physical_monitors {
                    let desc = pm.szPhysicalMonitorDescription;
                    let name = String::from_utf16_lossy(&desc)
                        .trim_matches(char::from(0))
                        .to_string();

                    monitors.push(Monitor {
                        hmonitor,
                        physical: pm.hPhysicalMonitor,
                        name,
                    });
                }
            }
        }
    }
    BOOL::from(true)
}
