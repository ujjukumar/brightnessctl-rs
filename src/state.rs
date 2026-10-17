use serde::{Serialize, Deserialize};
use windows::Win32::Foundation::HWND;
use crate::brightness::{self, PhysicalHandle};
use crate::settings::{Settings, MonitorState};
use crate::worker::{WriteResult, Worker};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Consecutive failed writes after which a monitor is disabled until the next refresh.
const MAX_FAILURES: u32 = 10;

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

/// How a monitor's brightness is controlled. Chosen once by `brightness::probe_all`.
#[derive(Debug, Clone, PartialEq)]
pub enum Backend {
    /// Not probed yet, or no API could read the monitor.
    Unsupported,
    /// Dxva2 high-level monitor API (DDC/CI underneath).
    HighLevel,
    /// Raw DDC/CI VCP code 0x10, for monitors whose capabilities string lacks brightness.
    Vcp { max: u32 },
    /// WMI, for built-in laptop panels.
    Wmi,
}

#[derive(Debug)]
pub struct Monitor {
    /// Dxva2 handle. Moves into the worker thread once it starts.
    pub physical: Option<PhysicalHandle>,
    pub hmonitor: isize,
    pub name: String,
    pub identity: Option<MonitorIdentity>,
    pub device_path: String,
    pub backend: Backend,
    pub hardware_min: u32,
    pub hardware_max: u32,
    /// Slider position, 0.0-1.0. Changes immediately on input.
    pub ui_value: f32,
    /// Last value (percent) the worker reported as written.
    pub written_value: Option<u32>,
    /// Last value (percent) sent to the worker, or known to be on the hardware.
    pub last_requested: Option<u32>,
    pub last_set_by_app: bool,
    pub write_confirmed: bool,
    pub timestamp: u64,
    pub failure_count: u32,
    pub is_disabled: bool,
    pub worker: Option<Worker>,
}

impl Monitor {
    pub fn new(
        physical: Option<PhysicalHandle>,
        hmonitor: isize,
        name: String,
        identity: Option<MonitorIdentity>,
        device_path: String,
    ) -> Self {
        Self {
            physical,
            hmonitor,
            name,
            identity,
            device_path,
            backend: Backend::Unsupported,
            hardware_min: 0,
            hardware_max: 100,
            ui_value: 0.5,
            written_value: None,
            last_requested: None,
            last_set_by_app: false,
            write_confirmed: false,
            timestamp: 0,
            failure_count: 0,
            is_disabled: false,
            worker: None,
        }
    }

    /// The slider value as a whole percentage.
    pub fn percent(&self) -> u32 {
        (self.ui_value.clamp(0.0, 1.0) * 100.0).round() as u32
    }

    /// Sends the slider value to the worker, unless it is the value last sent.
    pub fn request_write(&mut self) {
        let percent = self.percent();
        if self.is_disabled || self.last_requested == Some(percent) {
            return;
        }
        if let Some(worker) = &self.worker {
            worker.request(percent);
            self.last_requested = Some(percent);
        }
    }

    /// Records a worker's result. Returns true if anything shown in the label changed.
    pub fn apply_write_result(&mut self, result: WriteResult) -> bool {
        let before = (self.write_confirmed, self.is_disabled);
        if result.success {
            self.failure_count = 0;
            self.written_value = Some(result.value);
            self.last_set_by_app = true;
            self.timestamp = unix_timestamp();
            if let Some(confirmed) = result.confirmed {
                self.write_confirmed = confirmed;
            }
        } else {
            self.failure_count += 1;
            // Let the same value be requested again.
            self.last_requested = None;
            if self.failure_count >= MAX_FAILURES {
                self.is_disabled = true;
            }
        }
        before != (self.write_confirmed, self.is_disabled)
    }

    /// The state to persist, or `None` if the app never wrote this monitor.
    pub fn to_state(&self) -> Option<MonitorState> {
        let value = self.written_value?;
        Some(MonitorState {
            brightness_value: value,
            normalized_value: value as f32 / 100.0,
            last_set_by_app: self.last_set_by_app,
            write_confirmed: self.write_confirmed,
            timestamp: self.timestamp,
            last_seen_device_path: self.device_path.clone(),
        })
    }
}

fn unix_timestamp() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

use std::collections::HashMap;
use crate::actions::Action;

#[derive(Default)]
pub struct AppState {
    pub monitors: Vec<Monitor>,
    pub settings: Settings,
    pub status_message: String,
    pub hover_monitor_idx: Option<usize>,
    pub active_monitor_idx: Option<usize>,
    pub lock_mode: bool,
    pub hotkey_status: HashMap<Action, bool>,
    pub last_wheel_time: Option<Instant>,
    pub editing_monitor: Option<usize>,
    pub edit_buffer: String,
    /// Bumped whenever workers are restarted, to ignore results from older ones.
    pub worker_generation: u16,
    /// A monitor the app wrote has state not yet saved to the config file.
    pub settings_dirty: bool,
}

impl AppState {
    /// Applies a result from the worker of monitor `idx`. Results from an older worker
    /// generation are ignored. Returns true if the monitor's label changed.
    pub fn handle_write_result(&mut self, generation: u16, idx: usize, result: WriteResult) -> bool {
        if generation != self.worker_generation {
            return false;
        }
        let Some(m) = self.monitors.get_mut(idx) else {
            return false;
        };
        let label_changed = m.apply_write_result(result);
        if result.success && m.identity.is_some() {
            self.settings_dirty = true;
        }
        label_changed
    }

    /// Copies the state of every monitor the app wrote into the settings and clears the
    /// dirty flag. The caller saves the settings.
    pub fn persist_monitor_states(&mut self) {
        for m in &self.monitors {
            if let (Some(ident), Some(saved)) = (&m.identity, m.to_state()) {
                self.settings.set_monitor_state(ident, saved);
            }
        }
        self.settings_dirty = false;
    }

    /// Starts a worker for every monitor with a usable backend, reporting to `hwnd`.
    pub fn start_workers(&mut self, hwnd: HWND) {
        self.worker_generation = self.worker_generation.wrapping_add(1);
        let generation = self.worker_generation;
        for (index, m) in self.monitors.iter_mut().enumerate() {
            m.worker = brightness::worker_spec(m).and_then(|spec| Worker::spawn(spec, hwnd, generation, index));
        }
    }

    /// Sends the slider value of each target monitor to its worker. Every input path
    /// changes `ui_value` first and then calls this.
    pub fn request_brightness(&mut self, indices: &[usize]) {
        for &idx in indices {
            if let Some(m) = self.monitors.get_mut(idx) {
                m.request_write();
            }
        }
    }

    /// Moves every monitor by `delta`; each stops independently at 0.0 or 1.0.
    pub fn apply_sync_delta(&mut self, _dragged_idx: usize, delta: f32) {
        for m in &mut self.monitors {
            m.ui_value = (m.ui_value + delta).clamp(0.0, 1.0);
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
            if let Some(m) = self.monitors.get_mut(idx) {
                m.ui_value = (m.ui_value + delta).clamp(0.0, 1.0);
            }
        }
    }

    pub fn set_absolute(&mut self, indices: &[usize], value: f32) {
        let value = value.clamp(0.0, 1.0);
        for &idx in indices {
            if let Some(m) = self.monitors.get_mut(idx) {
                m.ui_value = value;
            }
        }
    }

    pub fn cycle_presets(&mut self, indices: &[usize]) {
        let presets = &self.settings.presets;
        if presets.is_empty() { return; }

        for &idx in indices {
            if let Some(m) = self.monitors.get_mut(idx) {
                // Next preset strictly greater than the current value, wrapping to the first.
                m.ui_value = presets
                    .iter()
                    .copied()
                    .find(|&p| p > m.ui_value + 0.001)
                    .unwrap_or(presets[0]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::Mailbox;
    use std::sync::Arc;

    fn monitor(value: f32) -> Monitor {
        let mut m = Monitor::new(None, 0, "Test".into(), None, String::new());
        m.ui_value = value;
        m
    }

    fn state_with(values: &[f32]) -> AppState {
        AppState {
            monitors: values.iter().map(|&v| monitor(v)).collect(),
            ..Default::default()
        }
    }

    fn values(state: &AppState) -> Vec<f32> {
        state.monitors.iter().map(|m| m.ui_value).collect()
    }

    fn assert_close(actual: &[f32], expected: &[f32]) {
        assert_eq!(actual.len(), expected.len());
        for (a, e) in actual.iter().zip(expected) {
            assert!((a - e).abs() < 0.001, "{actual:?} != {expected:?}");
        }
    }

    #[test]
    fn test_sync_delta_logic() {
        let mut state = state_with(&[0.1, 0.5, 0.9]);

        // +10%
        state.apply_sync_delta(0, 0.1);
        assert_close(&values(&state), &[0.2, 0.6, 1.0]);

        // +10% more (Saturation test)
        state.apply_sync_delta(0, 0.1);
        assert_close(&values(&state), &[0.3, 0.7, 1.0]);

        // -20% (Reverse movement test: moves on from 1.0, no catching up)
        state.apply_sync_delta(0, -0.2);
        assert_close(&values(&state), &[0.1, 0.5, 0.8]);
    }

    #[test]
    fn test_step_arithmetic() {
        let mut state = state_with(&[0.5]);

        state.apply_step(&[0], 0.1);
        assert_close(&values(&state), &[0.6]);

        state.apply_step(&[0], -0.2);
        assert_close(&values(&state), &[0.4]);

        // Clamp upper
        state.apply_step(&[0], 0.7);
        assert_eq!(values(&state), vec![1.0]);

        // Clamp lower
        state.apply_step(&[0], -1.2);
        assert_eq!(values(&state), vec![0.0]);

        // Out-of-range index is ignored
        state.apply_step(&[5], 0.1);
        assert_eq!(values(&state), vec![0.0]);
    }

    #[test]
    fn test_set_absolute() {
        let mut state = state_with(&[0.5, 0.5]);

        state.set_absolute(&[0], 0.8);
        assert_eq!(values(&state), vec![0.8, 0.5]);

        // Clamp
        state.set_absolute(&[1], 1.5);
        assert_eq!(values(&state), vec![0.8, 1.0]);

        state.set_absolute(&[1], -0.5);
        assert_eq!(values(&state), vec![0.8, 0.0]);
    }

    #[test]
    fn test_preset_cycling() {
        let mut state = state_with(&[0.0]);
        state.settings.presets = vec![0.0, 0.5, 1.0];

        state.cycle_presets(&[0]);
        assert_eq!(values(&state), vec![0.5]);

        state.cycle_presets(&[0]);
        assert_eq!(values(&state), vec![1.0]);

        state.cycle_presets(&[0]);
        assert_eq!(values(&state), vec![0.0]); // Wrap

        // Intermediate value
        state.monitors[0].ui_value = 0.3;
        state.cycle_presets(&[0]);
        assert_eq!(values(&state), vec![0.5]);

        // Empty presets
        state.settings.presets = vec![];
        state.monitors[0].ui_value = 0.7;
        state.cycle_presets(&[0]);
        assert_eq!(values(&state), vec![0.7]); // No-op
    }

    #[test]
    fn test_request_brightness_skips_unchanged_values() {
        let mut state = state_with(&[0.42]);
        let mailbox = Arc::new(Mailbox::default());
        state.monitors[0].worker = Some(Worker::detached(Arc::clone(&mailbox)));

        state.request_brightness(&[0]);
        assert_eq!(mailbox.take_pending(), Some(42));

        // Same value again: nothing is sent.
        state.request_brightness(&[0]);
        assert_eq!(mailbox.take_pending(), None);

        state.set_absolute(&[0], 0.43);
        state.request_brightness(&[0]);
        assert_eq!(mailbox.take_pending(), Some(43));
    }

    #[test]
    fn test_write_results_update_monitor() {
        let mut m = monitor(0.5);
        m.last_requested = Some(50);

        // An in-between write isn't checked, so the confirmation flag is left alone.
        assert!(!m.apply_write_result(WriteResult { value: 40, success: true, confirmed: None }));
        assert_eq!(m.written_value, Some(40));
        assert!(m.last_set_by_app);
        assert!(!m.write_confirmed);

        // The final write was read back: the label changes.
        assert!(m.apply_write_result(WriteResult { value: 50, success: true, confirmed: Some(true) }));
        assert!(m.write_confirmed);
        assert_eq!(m.to_state().unwrap().brightness_value, 50);
    }

    #[test]
    fn test_monitor_disabled_after_repeated_failures() {
        let mut m = monitor(0.5);
        let failure = WriteResult { value: 50, success: false, confirmed: None };

        for _ in 0..MAX_FAILURES - 1 {
            m.last_requested = Some(50);
            m.apply_write_result(failure);
            assert_eq!(m.last_requested, None, "a failed value can be retried");
        }
        assert!(!m.is_disabled);

        assert!(m.apply_write_result(failure));
        assert!(m.is_disabled);

        // A success resets the count.
        let mut m = monitor(0.5);
        m.apply_write_result(failure);
        m.apply_write_result(WriteResult { value: 50, success: true, confirmed: Some(true) });
        assert_eq!(m.failure_count, 0);
    }

    #[test]
    fn test_to_state_requires_a_write() {
        assert!(monitor(0.5).to_state().is_none());
    }

    fn identified(value: f32) -> Monitor {
        let mut m = monitor(value);
        m.identity = Some(MonitorIdentity {
            manufacturer_id: "GBT".into(),
            product_code: 9218,
            serial: "22350B003157".into(),
            is_fallback: false,
        });
        m
    }

    #[test]
    fn test_successful_write_marks_settings_dirty() {
        let mut state = AppState {
            monitors: vec![identified(0.4), monitor(0.6)],
            worker_generation: 3,
            ..Default::default()
        };
        let ok = |value| WriteResult { value, success: true, confirmed: Some(true) };

        // A failure, a stale generation, or a monitor without identity: nothing to save.
        state.handle_write_result(3, 0, WriteResult { value: 40, success: false, confirmed: None });
        state.handle_write_result(2, 0, ok(40));
        state.handle_write_result(3, 1, ok(60));
        state.handle_write_result(3, 7, ok(60));
        assert!(!state.settings_dirty);
        assert_eq!(state.monitors[0].written_value, None, "stale result ignored");

        state.handle_write_result(3, 0, ok(40));
        assert!(state.settings_dirty);
    }

    #[test]
    fn test_persist_saves_written_value_and_clears_dirty() {
        let mut state = AppState {
            monitors: vec![identified(0.4)],
            ..Default::default()
        };
        state.handle_write_result(0, 0, WriteResult { value: 40, success: true, confirmed: Some(true) });
        // The slider has already moved on; the saved value is what the hardware got.
        state.monitors[0].ui_value = 0.9;

        state.persist_monitor_states();
        assert!(!state.settings_dirty);
        let ident = state.monitors[0].identity.clone().unwrap();
        let saved = state.settings.get_monitor_state(&ident).unwrap();
        assert_eq!(saved.brightness_value, 40);
        assert!(saved.last_set_by_app && saved.write_confirmed);
    }

    #[test]
    fn test_unchanged_request_leaves_settings_clean() {
        let mut state = AppState {
            monitors: vec![identified(0.5)],
            ..Default::default()
        };
        state.monitors[0].last_requested = Some(50);
        state.request_brightness(&[0]);
        assert!(!state.settings_dirty);
        state.persist_monitor_states();
        assert!(state.settings.monitor_states.is_empty(), "a monitor the app never wrote isn't saved");
    }
}
