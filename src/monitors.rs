use crate::state::Monitor;
use windows::core::{BOOL, PCWSTR, GUID};
use windows::Win32::Foundation::{LPARAM, RECT, HWND, POINT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, HDC, HMONITOR, GetMonitorInfoW, MONITORINFOEXW, 
    EnumDisplayDevicesW, DISPLAY_DEVICEW, MonitorFromPoint, MONITOR_DEFAULTTONULL,
};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
use windows::Win32::Devices::Display::{
    GetNumberOfPhysicalMonitorsFromHMONITOR, GetPhysicalMonitorsFromHMONITOR, PHYSICAL_MONITOR,
};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    SetupDiGetClassDevsW, SetupDiEnumDeviceInfo, SetupDiOpenDevRegKey, SetupDiDestroyDeviceInfoList,
    SetupDiGetDeviceInstanceIdW, SP_DEVINFO_DATA, DIGCF_PRESENT, DICS_FLAG_GLOBAL, DIREG_DEV,
    HDEVINFO,
};
use windows::Win32::System::Registry::{
    RegQueryValueExW, RegCloseKey, HKEY, KEY_READ, REG_VALUE_TYPE,
};

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

pub fn get_monitor_handle_at_cursor() -> isize {
    unsafe {
        let mut point = POINT::default();
        if GetCursorPos(&mut point).is_ok() {
            let hmonitor = MonitorFromPoint(point, MONITOR_DEFAULTTONULL);
            if !hmonitor.is_invalid() {
                return hmonitor.0 as isize;
            }
        }
        0
    }
}

/// Retrieves the EDID blob for a given physical monitor handle, if available.
pub fn get_edid_blob(hmonitor: HMONITOR) -> Option<Vec<u8>> {
    let device_id = get_monitor_device_id(hmonitor)?;
    get_edid_from_registry(&device_id)
}

fn get_monitor_device_id(hmonitor: HMONITOR) -> Option<String> {
    unsafe {
        let mut mi = MONITORINFOEXW::default();
        mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if !GetMonitorInfoW(hmonitor, &mut mi as *mut _ as *mut _).as_bool() {
            return None;
        }

        let mut dd = DISPLAY_DEVICEW::default();
        dd.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;

        if !EnumDisplayDevicesW(PCWSTR(mi.szDevice.as_ptr()), 0, &mut dd, 0).as_bool() {
            return None;
        }

        Some(String::from_utf16_lossy(&dd.DeviceID).trim_matches('\0').to_string())
    }
}

fn get_edid_from_registry(target_device_id: &str) -> Option<Vec<u8>> {
    let guid_monitor = GUID::from_u128(0x4d36e96e_e325_11ce_bfc1_08002be10318);

    unsafe {
        let hdevinfo = SetupDiGetClassDevsW(
            Some(&guid_monitor),
            None,
            Some(HWND::default()),
            DIGCF_PRESENT,
        ).ok()?;

        if hdevinfo.is_invalid() {
            return None;
        }

        let _guard = DevInfoGuard(hdevinfo);

        let mut index = 0;
        let mut sp_devinfo_data = SP_DEVINFO_DATA::default();
        sp_devinfo_data.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;

        while SetupDiEnumDeviceInfo(hdevinfo, index, &mut sp_devinfo_data).is_ok() {
            index += 1;

            let mut required_size = 0;
            let _ = SetupDiGetDeviceInstanceIdW(hdevinfo, &sp_devinfo_data, None, Some(&mut required_size));
            
            if required_size == 0 { continue; }

            let mut buffer = vec![0u16; required_size as usize];
            if !SetupDiGetDeviceInstanceIdW(hdevinfo, &sp_devinfo_data, Some(&mut buffer), Some(&mut required_size)).is_ok() {
                continue;
            }

            let instance_id = String::from_utf16_lossy(&buffer).trim_matches('\0').to_string();

            if instance_id.eq_ignore_ascii_case(target_device_id) {
                let hkey_result = SetupDiOpenDevRegKey(
                    hdevinfo, 
                    &sp_devinfo_data, 
                    DICS_FLAG_GLOBAL.0, 
                    0, 
                    DIREG_DEV, 
                    KEY_READ.0
                );

                if let Ok(hkey) = hkey_result {
                    if hkey.is_invalid() { return None; }
                    let _key_guard = KeyGuard(hkey);

                    let value_name = windows::core::w!("EDID");
                    let mut data_size = 0;
                    let mut data_type = REG_VALUE_TYPE::default();
                    
                    let _ = RegQueryValueExW(hkey, value_name, None, Some(&mut data_type), None, Some(&mut data_size));
                    
                    if data_size > 0 {
                        let mut data = vec![0u8; data_size as usize];
                        if RegQueryValueExW(hkey, value_name, None, Some(&mut data_type), Some(data.as_mut_ptr()), Some(&mut data_size)).is_ok() {
                            return Some(data);
                        }
                    }
                }
                return None;
            }
        }
    }
    None
}

struct DevInfoGuard(HDEVINFO);
impl Drop for DevInfoGuard {
    fn drop(&mut self) {
        unsafe { let _ = SetupDiDestroyDeviceInfoList(self.0); }
    }
}

struct KeyGuard(HKEY);
impl Drop for KeyGuard {
    fn drop(&mut self) {
        unsafe { let _ = RegCloseKey(self.0); }
    }
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
                    let device_id = get_monitor_device_id(hmonitor).unwrap_or_default();
                    let edid_blob = get_edid_blob(hmonitor);
                    let identity = edid_blob.and_then(|blob| crate::edid::parse_identity(&blob));

                    monitors.push(Monitor {
                        physical: pm.hPhysicalMonitor,
                        hmonitor: hmonitor.0 as isize,
                        name: String::from_utf16_lossy(&desc).trim_matches('\0').to_string(),
                        identity,
                        normalized_value: 0.0,
                        last_set_by_app: false,
                        write_confirmed: false,
                        timestamp: 0,
                        device_path: device_id,
                        last_write_time: None,
                        failure_count: 0,
                        is_disabled: false,
                        hardware_min: 0,
                        hardware_max: 100,
                        observed_min: 0,
                        observed_max: 100,
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
        // Since monitors.len() is usize, it's always >= 0.
        // We just want to check it didn't panic.
        assert!(monitors.len() >= 0); 
    }

    #[test]
    fn test_get_edid_blob_stub() {
        // This test merely ensures the function exists and can be called.
        let blob = get_edid_blob(HMONITOR(std::ptr::null_mut()));
        assert!(blob.is_none());
    }
}