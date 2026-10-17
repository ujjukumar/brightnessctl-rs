use crate::brightness::PhysicalHandle;
use crate::state::Monitor;
use windows::core::{BOOL, PCWSTR, GUID};
use windows::Win32::Foundation::{HANDLE, LPARAM, RECT, HWND, POINT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, HDC, HMONITOR, GetMonitorInfoW, MONITORINFO, MONITORINFOEXW,
    EnumDisplayDevicesW, DISPLAY_DEVICEW, DISPLAY_DEVICE_ACTIVE, MonitorFromPoint, MONITOR_DEFAULTTONULL,
};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, EDD_GET_DEVICE_INTERFACE_NAME};
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

/// Retrieves the EDID blob for a monitor device instance ID
/// (e.g. `DISPLAY\GBT2402\5&145fde2&5&UID24837`), if available.
pub fn get_edid_blob(instance_id: &str) -> Option<Vec<u8>> {
    if instance_id.is_empty() {
        return None;
    }
    get_edid_from_registry(instance_id)
}

/// Converts a monitor device interface path into its device instance ID.
///
/// `\\?\DISPLAY#GBT2402#5&145fde2&5&UID24837#{e6f07b5f-...}` becomes
/// `DISPLAY\GBT2402\5&145fde2&5&UID24837`, which matches the IDs reported by
/// SetupDi when compared case-insensitively.
pub fn interface_path_to_instance_id(path: &str) -> Option<String> {
    let path = path.trim_end_matches('\0');
    let path = path.strip_prefix(r"\\?\").unwrap_or(path);
    let guid_start = path.rfind("#{")?;
    let id = &path[..guid_start];
    if id.is_empty() {
        return None;
    }
    Some(id.replace('#', "\\"))
}

fn from_wide_nul(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// An active monitor device attached to a display output.
struct DisplayDevice {
    instance_id: String,
    name: String,
}

/// Returns the active monitor devices attached to `hmonitor`, in the order
/// Windows enumerates them.
fn get_display_devices(hmonitor: HMONITOR) -> Vec<DisplayDevice> {
    let mut devices = Vec::new();
    unsafe {
        let mut mi = MONITORINFOEXW {
            monitorInfo: MONITORINFO {
                cbSize: size_of::<MONITORINFOEXW>() as u32,
                ..Default::default()
            },
            ..Default::default()
        };
        if !GetMonitorInfoW(hmonitor, &mut mi as *mut _ as *mut _).as_bool() {
            return devices;
        }

        let mut index = 0;
        loop {
            let mut dd = DISPLAY_DEVICEW {
                cb: size_of::<DISPLAY_DEVICEW>() as u32,
                ..Default::default()
            };
            if !EnumDisplayDevicesW(PCWSTR(mi.szDevice.as_ptr()), index, &mut dd, EDD_GET_DEVICE_INTERFACE_NAME).as_bool() {
                break;
            }
            index += 1;

            if dd.StateFlags.0 & DISPLAY_DEVICE_ACTIVE.0 == 0 {
                continue;
            }
            if let Some(instance_id) = interface_path_to_instance_id(&from_wide_nul(&dd.DeviceID)) {
                devices.push(DisplayDevice {
                    instance_id,
                    name: from_wide_nul(&dd.DeviceString),
                });
            }
        }
    }
    devices
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
        let mut sp_devinfo_data = SP_DEVINFO_DATA {
            cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };

        while SetupDiEnumDeviceInfo(hdevinfo, index, &mut sp_devinfo_data).is_ok() {
            index += 1;

            let mut required_size = 0;
            let _ = SetupDiGetDeviceInstanceIdW(hdevinfo, &sp_devinfo_data, None, Some(&mut required_size));
            
            if required_size == 0 { continue; }

            let mut buffer = vec![0u16; required_size as usize];
            if SetupDiGetDeviceInstanceIdW(hdevinfo, &sp_devinfo_data, Some(&mut buffer), Some(&mut required_size)).is_err() {
                continue;
            }

            let instance_id = from_wide_nul(&buffer);

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
        let devices = get_display_devices(hmonitor);

        let mut physical_monitors = Vec::new();
        let mut number_of_physical_monitors = 0;
        if GetNumberOfPhysicalMonitorsFromHMONITOR(hmonitor, &mut number_of_physical_monitors).is_ok() && number_of_physical_monitors > 0 {
            let mut buffer = vec![PHYSICAL_MONITOR::default(); number_of_physical_monitors as usize];
            if GetPhysicalMonitorsFromHMONITOR(hmonitor, &mut buffer).is_ok() {
                physical_monitors = buffer;
            }
        }

        if physical_monitors.is_empty() {
            // Without a Dxva2 handle a built-in panel can still be controlled through WMI.
            for device in devices {
                monitors.push(new_monitor(None, hmonitor, device.name, device.instance_id));
            }
            return BOOL::from(true);
        }

        // With cloned displays one HMONITOR has several physical monitors. Windows offers
        // no direct link between a physical monitor and its display device, so pair by index.
        for (i, pm) in physical_monitors.into_iter().enumerate() {
            let desc = pm.szPhysicalMonitorDescription; // copy out of the packed struct
            let device_path = devices.get(i).map(|d| d.instance_id.clone()).unwrap_or_default();
            monitors.push(new_monitor(Some(pm.hPhysicalMonitor), hmonitor, from_wide_nul(&desc), device_path));
        }
    }
    BOOL::from(true)
}

fn new_monitor(physical: Option<HANDLE>, hmonitor: HMONITOR, name: String, device_path: String) -> Monitor {
    let identity = get_edid_blob(&device_path).and_then(|blob| crate::edid::parse_identity(&blob));
    Monitor::new(physical.map(PhysicalHandle::new), hmonitor.0 as isize, name, identity, device_path)
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
    fn test_get_edid_blob_unknown_id() {
        assert!(get_edid_blob("").is_none());
        assert!(get_edid_blob(r"DISPLAY\NOPE0000\0&0&0&UID0").is_none());
    }

    #[test]
    fn test_interface_path_to_instance_id() {
        let gbt = interface_path_to_instance_id(
            r"\\?\DISPLAY#GBT2402#5&145fde2&5&UID24837#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}",
        );
        assert_eq!(gbt.as_deref(), Some(r"DISPLAY\GBT2402\5&145fde2&5&UID24837"));

        let acr = interface_path_to_instance_id(
            "\\\\?\\DISPLAY#ACR0B96#5&145fde2&5&UID24832#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}\0\0\0",
        );
        assert_eq!(acr.as_deref(), Some(r"DISPLAY\ACR0B96\5&145fde2&5&UID24832"));
    }

    #[test]
    fn test_instance_id_matches_setupdi_format_ignoring_case() {
        let id = interface_path_to_instance_id(
            r"\\?\DISPLAY#GBT2402#5&145fde2&5&UID24837#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}",
        )
        .unwrap();
        // SetupDi reports the same ID with an upper-case instance part.
        assert!(id.eq_ignore_ascii_case(r"DISPLAY\GBT2402\5&145FDE2&5&UID24837"));
    }

    #[test]
    fn test_interface_path_to_instance_id_rejects_malformed() {
        assert_eq!(interface_path_to_instance_id(""), None);
        assert_eq!(interface_path_to_instance_id(r"\\?\DISPLAY#GBT2402#5&145fde2"), None);
        assert_eq!(interface_path_to_instance_id(r"\\?\#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}"), None);
    }

    #[test]
    #[ignore = "requires connected monitors with EDID"]
    fn test_enumerated_monitors_have_identity() {
        let monitors = enumerate_monitors();
        assert!(!monitors.is_empty());
        for m in &monitors {
            println!("{} | {:?} | {} | {:?}", m.name, m.physical.as_ref().map(|h| h.raw()), m.device_path, m.identity.as_ref().map(|i| i.composite_key()));
            assert!(!m.device_path.is_empty(), "no device path for {}", m.name);
            assert!(m.identity.is_some(), "no identity for {} ({})", m.name, m.device_path);
        }
    }
}