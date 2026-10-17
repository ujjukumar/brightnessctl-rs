//! WMI brightness backend for built-in laptop panels, which don't support DDC/CI.
//!
//! Reads `WmiMonitorBrightness.CurrentBrightness` and writes through
//! `WmiMonitorBrightnessMethods.WmiSetBrightness` in the `root\WMI` namespace.
//! Both work in percent (0-100); the driver snaps writes to the panel's supported levels.

use std::ffi::c_void;
use windows::core::{w, IUnknown, Param, BSTR, Result, PCWSTR};
use windows::Win32::System::Com::{
    CoCreateInstance, CoSetProxyBlanket, CLSCTX_INPROC_SERVER, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL,
    RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Variant::{
    VariantClear, VARIANT, VT_BSTR, VT_I2, VT_I4, VT_UI1, VT_UI2, VT_UI4,
};
use windows::Win32::System::Wmi::{
    IEnumWbemClassObject, IWbemClassObject, IWbemLocator, IWbemServices, WbemLocator,
    WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_GENERIC_FLAG_TYPE, WBEM_INFINITE,
};

const RPC_C_AUTHN_WINNT: u32 = 10;
const RPC_C_AUTHZ_NONE: u32 = 0;

/// A connection to the `root\WMI` namespace. Must be used on the thread that created it.
#[derive(Debug, Clone)]
pub struct Connection {
    services: IWbemServices,
}

/// A built-in panel controllable through WMI.
#[derive(Debug, Clone)]
pub struct Panel {
    services: IWbemServices,
    instance_name: String,
    method_path: BSTR,
    in_params: IWbemClassObject,
}

impl Connection {
    /// Connects to `root\WMI`. COM must already be initialized on this thread.
    pub fn connect() -> Result<Self> {
        unsafe {
            let locator: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER)?;
            let services = locator.ConnectServer(
                &BSTR::from(r"ROOT\WMI"),
                &BSTR::new(),
                &BSTR::new(),
                &BSTR::new(),
                0,
                &BSTR::new(),
                None,
            )?;
            set_proxy_blanket(&services)?;
            Ok(Self { services })
        }
    }

    /// Finds the WMI-controllable panel for a monitor device instance ID.
    pub fn find_panel(&self, device_path: &str) -> Option<Panel> {
        let methods = self.instances("WmiMonitorBrightnessMethods").ok()?;
        let (instance_name, target) = methods.into_iter().find_map(|obj| {
            let name = get(&obj, w!("InstanceName"))?.as_string()?;
            instance_matches(&name, device_path).then_some((name, obj))
        })?;
        let method_path = BSTR::from(get(&target, w!("__PATH"))?.as_string()?);

        unsafe {
            let mut class = None;
            self.services
                .GetObject(
                    &BSTR::from("WmiMonitorBrightnessMethods"),
                    WBEM_GENERIC_FLAG_TYPE(0),
                    None,
                    Some(&mut class),
                    None,
                )
                .ok()?;
            let mut in_signature = None;
            class?
                .GetMethod(w!("WmiSetBrightness"), 0, &mut in_signature, std::ptr::null_mut())
                .ok()?;
            let in_params = in_signature?.SpawnInstance(0).ok()?;
            // CIM uint32 parameters are passed as VT_I4.
            in_params.Put(w!("Timeout"), 0, &Variant::i4(0).0, 0).ok()?;

            Some(Panel {
                services: self.services.clone(),
                instance_name,
                method_path,
                in_params,
            })
        }
    }

    fn instances(&self, class: &str) -> Result<Vec<IWbemClassObject>> {
        query(&self.services, class)
    }
}

impl Panel {
    /// Current brightness in percent.
    pub fn get_brightness(&self) -> Option<u32> {
        query(&self.services, "WmiMonitorBrightness")
            .ok()?
            .into_iter()
            .find(|obj| {
                get(obj, w!("InstanceName"))
                    .and_then(|v| v.as_string())
                    .is_some_and(|name| name.eq_ignore_ascii_case(&self.instance_name))
            })
            .and_then(|obj| get(&obj, w!("CurrentBrightness"))?.as_u32())
    }

    /// Sets brightness in percent (clamped to 100).
    pub fn set_brightness(&self, percent: u32) -> bool {
        unsafe {
            let value = Variant::ui1(percent.min(100) as u8);
            if self.in_params.Put(w!("Brightness"), 0, &value.0, 0).is_err() {
                return false;
            }
            self.services
                .ExecMethod(
                    &self.method_path,
                    &BSTR::from("WmiSetBrightness"),
                    WBEM_GENERIC_FLAG_TYPE(0),
                    None,
                    &self.in_params,
                    None,
                    None,
                )
                .is_ok()
        }
    }
}

/// True if a WMI `InstanceName` (device instance ID plus a `_N` suffix, e.g.
/// `DISPLAY\BOE0900\4&...&UID265988_0`) belongs to the monitor with `device_path`.
pub fn instance_matches(instance_name: &str, device_path: &str) -> bool {
    if device_path.is_empty() {
        return false;
    }
    let base = match instance_name.rsplit_once('_') {
        Some((base, suffix)) if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) => base,
        _ => instance_name,
    };
    base.eq_ignore_ascii_case(device_path)
}

fn query(services: &IWbemServices, class: &str) -> Result<Vec<IWbemClassObject>> {
    unsafe {
        let enumerator: IEnumWbemClassObject = services.ExecQuery(
            &BSTR::from("WQL"),
            &BSTR::from(format!("SELECT * FROM {class}")),
            WBEM_GENERIC_FLAG_TYPE(WBEM_FLAG_FORWARD_ONLY.0 | WBEM_FLAG_RETURN_IMMEDIATELY.0),
            None,
        )?;
        set_proxy_blanket(&enumerator)?;

        let mut objects = Vec::new();
        loop {
            let mut batch = [None];
            let mut returned = 0;
            enumerator.Next(WBEM_INFINITE, &mut batch, &mut returned).ok()?;
            if returned == 0 {
                break;
            }
            objects.extend(batch[0].take());
        }
        Ok(objects)
    }
}

/// WMI lives in a separate service; calls need impersonation rights. The blanket
/// applies per interface proxy, so pass the interface itself, never a cast `IUnknown`.
fn set_proxy_blanket<P: Param<IUnknown>>(proxy: P) -> Result<()> {
    unsafe {
        CoSetProxyBlanket(
            proxy,
            RPC_C_AUTHN_WINNT,
            RPC_C_AUTHZ_NONE,
            PCWSTR::null(),
            RPC_C_AUTHN_LEVEL_CALL,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            None::<*const c_void>,
            EOAC_NONE,
        )
    }
}

fn get(obj: &IWbemClassObject, name: PCWSTR) -> Option<Variant> {
    let mut value = Variant::empty();
    unsafe { obj.Get(name, 0, &mut value.0, None, None).ok()? };
    Some(value)
}

/// Owns a `VARIANT` and frees it on drop.
struct Variant(VARIANT);

impl Variant {
    fn empty() -> Self {
        Self(VARIANT::default())
    }

    fn i4(value: i32) -> Self {
        let mut v = Self::empty();
        unsafe {
            let inner = &mut *v.0.Anonymous.Anonymous;
            inner.vt = VT_I4;
            inner.Anonymous.lVal = value;
        }
        v
    }

    fn ui1(value: u8) -> Self {
        let mut v = Self::empty();
        unsafe {
            let inner = &mut *v.0.Anonymous.Anonymous;
            inner.vt = VT_UI1;
            inner.Anonymous.bVal = value;
        }
        v
    }

    fn as_u32(&self) -> Option<u32> {
        unsafe {
            let inner = &*self.0.Anonymous.Anonymous;
            match inner.vt {
                VT_UI1 => Some(inner.Anonymous.bVal as u32),
                VT_UI2 => Some(inner.Anonymous.uiVal as u32),
                VT_UI4 => Some(inner.Anonymous.ulVal),
                VT_I2 => u32::try_from(inner.Anonymous.iVal).ok(),
                VT_I4 => u32::try_from(inner.Anonymous.lVal).ok(),
                _ => None,
            }
        }
    }

    fn as_string(&self) -> Option<String> {
        unsafe {
            let inner = &*self.0.Anonymous.Anonymous;
            (inner.vt == VT_BSTR).then(|| inner.Anonymous.bstrVal.to_string())
        }
    }
}

impl Drop for Variant {
    fn drop(&mut self) {
        unsafe {
            let _ = VariantClear(&mut self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    #[test]
    fn test_instance_matches() {
        let device = r"DISPLAY\BOE0900\4&2a9b8b3b&0&UID265988";
        assert!(instance_matches(r"DISPLAY\BOE0900\4&2a9b8b3b&0&UID265988_0", device));
        assert!(instance_matches(r"display\boe0900\4&2A9B8B3B&0&uid265988_0", device));
        assert!(instance_matches(r"DISPLAY\BOE0900\4&2a9b8b3b&0&UID265988", device));
        assert!(!instance_matches(r"DISPLAY\BOE0900\4&2a9b8b3b&0&UID265989_0", device));
        assert!(!instance_matches(r"DISPLAY\BOE0900\4&2a9b8b3b&0&UID265988_x", device));
        assert!(!instance_matches(r"DISPLAY\BOE0900\4&2a9b8b3b&0&UID265988_0", ""));
    }

    #[test]
    fn test_variant_conversions() {
        assert_eq!(Variant::i4(42).as_u32(), Some(42));
        assert_eq!(Variant::i4(-1).as_u32(), None);
        assert_eq!(Variant::ui1(100).as_u32(), Some(100));
        assert_eq!(Variant::ui1(7).as_string(), None);
        assert_eq!(Variant::empty().as_u32(), None);
    }

    #[test]
    #[ignore = "requires WMI and connected monitors"]
    fn test_wmi_instances_match_enumerated_monitors() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let conn = Connection::connect().expect("connect to root\\WMI");
        // WmiMonitorID exists on desktops too and uses the same InstanceName format
        // as WmiMonitorBrightness.
        let ids = conn.instances("WmiMonitorID").expect("query WmiMonitorID");
        let names: Vec<String> = ids
            .iter()
            .filter_map(|obj| get(obj, w!("InstanceName"))?.as_string())
            .collect();
        println!("WmiMonitorID instances: {names:?}");

        for m in crate::monitors::enumerate_monitors() {
            assert!(
                names.iter().any(|n| instance_matches(n, &m.device_path)),
                "no WMI instance for {}",
                m.device_path
            );
            if let Some(panel) = conn.find_panel(&m.device_path) {
                println!("{} -> WMI panel at {:?}%", m.device_path, panel.get_brightness());
            } else {
                println!("{} -> no WMI brightness control", m.device_path);
            }
        }
    }
}
