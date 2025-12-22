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
    pub hmonitor: isize,
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

use std::collections::HashMap;
use crate::actions::Action;

#[derive(Default)]
pub struct AppState {
    pub monitors: Vec<Monitor>,
    pub brightness: Vec<f32>, // Normalized 0.0 - 1.0
    pub settings: Settings,
    pub status_message: String,
    pub hover_monitor_idx: Option<usize>,
    pub active_monitor_idx: Option<usize>,
    pub lock_mode: bool,
    pub hotkey_status: HashMap<Action, bool>,
}

impl AppState {
    pub fn apply_sync_delta(&mut self, _dragged_idx: usize, delta: f32) {
        for i in 0..self.brightness.len() {
            let target_pct = (self.brightness[i] + delta).clamp(0.0, 1.0);
            self.brightness[i] = target_pct;
        }
    }

    /// Determines which monitors should be affected based on current state and targeting rules.
    pub fn get_target_indices(&self) -> Vec<usize> {
        if self.lock_mode {
            // Lock Mode Active -> Global (all monitors)
            (0..self.monitors.len()).collect()
        } else {
            // Lock Mode Inactive
            // If hover -> that monitor
            if let Some(idx) = self.hover_monitor_idx {
                vec![idx]
            } else if let Some(idx) = self.active_monitor_idx {
                // If focus (active) -> that monitor
                vec![idx]
            } else if !self.monitors.is_empty() {
                // Else -> Primary (deterministic fallback to 0)
                vec![0]
            } else {
                vec![]
            }
        }
    }

    pub fn apply_step(&mut self, indices: &[usize], delta: f32) {
        for &idx in indices {
            if idx < self.brightness.len() {
                let old_val = self.brightness[idx];
                let new_val = (old_val + delta).clamp(0.0, 1.0);
                
                // Idempotency check
                if (new_val - old_val).abs() > 0.0001 {
                    self.brightness[idx] = new_val;
                }
            }
        }
    }

    pub fn set_absolute(&mut self, indices: &[usize], value: f32) {
        let value = value.clamp(0.0, 1.0);
        for &idx in indices {
            if idx < self.brightness.len() {
                // Idempotency check
                if (self.brightness[idx] - value).abs() > 0.0001 {
                    self.brightness[idx] = value;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_delta_logic() {
        let mut state = AppState::default();
        state.brightness = vec![0.1, 0.5, 0.9];
        
        // +10%
        state.apply_sync_delta(0, 0.1);
        assert!((state.brightness[0] - 0.2).abs() < 0.001);
        assert!((state.brightness[1] - 0.6).abs() < 0.001);
        assert!((state.brightness[2] - 1.0).abs() < 0.001);

        // +10% more (Saturation test)
        state.apply_sync_delta(0, 0.1);
        assert!((state.brightness[0] - 0.3).abs() < 0.001);
        assert!((state.brightness[1] - 0.7).abs() < 0.001);
        assert!((state.brightness[2] - 1.0).abs() < 0.001); // Stay at 1.0

        // -20% (Reverse movement test)
        state.apply_sync_delta(0, -0.2);
        assert!((state.brightness[0] - 0.1).abs() < 0.001);
        assert!((state.brightness[1] - 0.5).abs() < 0.001);
        assert!((state.brightness[2] - 0.8).abs() < 0.001); // Move from 1.0
    }

    #[test]
    fn test_step_arithmetic() {
        let mut state = AppState::default();
        state.brightness = vec![0.5];
        
        // Step up
        state.apply_step(&[0], 0.1);
        assert!((state.brightness[0] - 0.6).abs() < 0.001);

        // Step down
        state.apply_step(&[0], -0.2);
        assert!((state.brightness[0] - 0.4).abs() < 0.001);

        // Clamp upper
        state.apply_step(&[0], 0.7);
        assert_eq!(state.brightness[0], 1.0);

        // Clamp lower
        state.apply_step(&[0], -1.2);
        assert_eq!(state.brightness[0], 0.0);
        
        // Idempotency
        state.apply_step(&[0], 0.0);
        assert_eq!(state.brightness[0], 0.0);
    }

    #[test]
    fn test_set_absolute() {
        let mut state = AppState::default();
        state.brightness = vec![0.5, 0.5];

        state.set_absolute(&[0], 0.8);
        assert_eq!(state.brightness[0], 0.8);
        assert_eq!(state.brightness[1], 0.5);

        // Clamp
        state.set_absolute(&[1], 1.5);
        assert_eq!(state.brightness[1], 1.0);

        state.set_absolute(&[1], -0.5);
        assert_eq!(state.brightness[1], 0.0);
    }

    #[test]
    fn test_targeting_rules() {
        let mut state = AppState::default();
        // Add dummy monitors
        for _ in 0..3 {
            state.monitors.push(Monitor {
                physical: HANDLE(std::ptr::null_mut()),
                hmonitor: 0,
                name: "Test".to_string(),
                identity: None,
                normalized_value: 0.5,
                last_set_by_app: false,
                write_confirmed: false,
                timestamp: 0,
                device_path: "".to_string(),
                last_write_time: None,
                failure_count: 0,
                is_disabled: false,
                hardware_min: 0,
                hardware_max: 100,
                observed_min: 0,
                observed_max: 100,
            });
        }

        // Case 1: Lock mode active -> Global
        state.lock_mode = true;
        assert_eq!(state.get_target_indices(), vec![0, 1, 2]);

        // Case 2: Lock mode inactive, hover exists -> Hovered monitor
        state.lock_mode = false;
        state.hover_monitor_idx = Some(1);
        assert_eq!(state.get_target_indices(), vec![1]);

        // Case 3: Lock mode inactive, no hover, active monitor exists -> Active monitor
        state.hover_monitor_idx = None;
        state.active_monitor_idx = Some(2);
        assert_eq!(state.get_target_indices(), vec![2]);

        // Case 4: Lock mode inactive, no hover, no active -> Primary (0)
        state.active_monitor_idx = None;
        assert_eq!(state.get_target_indices(), vec![0]);

        // Case 5: Empty monitors
        state.monitors.clear();
        assert_eq!(state.get_target_indices(), Vec::<usize>::new());
    }
}