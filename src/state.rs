use windows::Win32::Graphics::Gdi::HMONITOR;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Devices::Display::{DestroyPhysicalMonitors, PHYSICAL_MONITOR};

#[derive(Debug)]
pub struct Monitor {
    pub hmonitor: HMONITOR,
    pub physical: HANDLE,
    pub name: String,
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

#[derive(Default)]
pub struct AppState {
    pub monitors: Vec<Monitor>,
    pub brightness: Vec<u32>,
    pub hot_monitor: Option<usize>,
}
