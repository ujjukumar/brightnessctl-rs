use windows::Win32::Foundation::HANDLE;
use windows::Win32::Devices::Display::{DestroyPhysicalMonitors, PHYSICAL_MONITOR};
use serde::{Serialize, Deserialize};
use crate::settings::{Settings, MonitorState};
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MonitorIdentity {
    pub manufacturer_id: String,
    pub product_code: u16,
    pub serial: String,
    pub is_fallback: bool,
}

impl MonitorIdentity {
    pub fn composite_key(&self) -> String {
        format!("{}:{}:{}", self.manufacturer_id, self.product_code, self.serial)
    }
}

#[derive(Debug)]
pub struct Monitor {
    pub physical: HANDLE,
    pub name: String,
    pub identity: Option<MonitorIdentity>,
    pub normalized_value: f32,
    pub last_set_by_app: bool,
    pub write_confirmed: bool,
    pub timestamp: u64,
    pub device_path: String,
    pub last_write_time: Option<Instant>,
    pub failure_count: u32,
    pub is_disabled: bool,
    pub hardware_min: u32,
    pub hardware_max: u32,
    pub observed_min: u32,
    pub observed_max: u32,
}

impl Monitor {
    pub fn to_state(&self, brightness_value: u32) -> MonitorState {
        MonitorState {
            brightness_value,
            normalized_value: self.normalized_value,
            last_set_by_app: self.last_set_by_app,
            write_confirmed: self.write_confirmed,
            timestamp: self.timestamp,
            last_seen_device_path: self.device_path.clone(),
        }
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        unsafe {
            if !self.physical.is_invalid() {
                let mut pm = PHYSICAL_MONITOR::default();
                pm.hPhysicalMonitor = self.physical;
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
    pub settings: Settings,
    pub status_message: String,
    pub hover_monitor_idx: Option<usize>,
    pub active_monitor_idx: Option<usize>,
    pub lock_mode: bool,
}

impl AppState {
    pub fn apply_sync_delta(&mut self, _dragged_idx: usize, delta: f32) {
        for i in 0..self.brightness.len() {
            let cur_pct = self.brightness[i] as f32 / 100.0;
            let target_pct = (cur_pct + delta).clamp(0.0, 1.0);
            self.brightness[i] = (target_pct * 100.0) as u32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_delta_logic() {
        let mut state = AppState::default();
        state.brightness = vec![10, 50, 90];
        
        // +10%
        state.apply_sync_delta(0, 0.1);
        assert_eq!(state.brightness[0], 20);
        assert_eq!(state.brightness[1], 60);
        assert_eq!(state.brightness[2], 100);

        // +10% more (Saturation test)
        state.apply_sync_delta(0, 0.1);
        assert_eq!(state.brightness[0], 30);
        assert_eq!(state.brightness[1], 70);
        assert_eq!(state.brightness[2], 100); // Stay at 100

        // -20% (Reverse movement test)
        state.apply_sync_delta(0, -0.2);
        assert_eq!(state.brightness[0], 10);
        assert_eq!(state.brightness[1], 50);
        assert_eq!(state.brightness[2], 80); // Move from 100
    }
}