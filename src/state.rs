use windows::Win32::Foundation::HANDLE;
use windows::Win32::Devices::Display::{DestroyPhysicalMonitors, PHYSICAL_MONITOR};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MonitorIdentity {
    pub manufacturer_id: String,
    pub product_code: u16,
    pub serial: String,
    pub is_fallback: bool,
}

#[derive(Debug)]
pub struct Monitor {
    pub physical: HANDLE,
    pub name: String,
    pub identity: Option<MonitorIdentity>, // Added identity
}

impl Drop for Monitor {
    fn drop(&mut self) {
        unsafe {
            if !self.physical.is_invalid() {
                let mut pm = PHYSICAL_MONITOR::default();
                pm.hPhysicalMonitor = self.physical;
                // Try passing slice, assuming idiomatic windows-rs
                let _ = DestroyPhysicalMonitors(&[pm]); 
            }
        }
    }
}

unsafe impl Send for Monitor {}
unsafe impl Sync for Monitor {}

use crate::settings::Settings;

#[derive(Default)]
pub struct AppState {
    pub monitors: Vec<Monitor>,
    pub brightness: Vec<u32>,
    pub settings: Settings,
    pub status_message: String,
    pub hover_monitor_idx: Option<usize>,
    pub active_monitor_idx: Option<usize>,
}
