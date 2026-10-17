use serde::{Serialize, Deserialize};
use windows::Win32::Foundation::HWND;
use crate::brightness::{self, PhysicalHandle};
use crate::settings::{Settings, MonitorState};
use crate::worker::{MonitorReady, WorkerJob, WriteResult, Worker};
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

/// How a monitor's brightness is controlled. Chosen once by the monitor's worker
/// (`brightness::with_probed_backend`).
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
    /// False until the worker has probed the monitor; until then input is ignored.
    pub ready: bool,
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
            ready: false,
            worker: None,
        }
    }

    /// True if input should change this monitor.
    pub fn accepts_input(&self) -> bool {
        self.ready && self.backend != Backend::Unsupported && !self.is_disabled
    }

    /// How the monitor is controlled, as shown next to its name.
    pub fn status_label(&self) -> &'static str {
        if !self.ready {
            return "Detecting\u{2026}";
        }
        if self.is_disabled {
            return "Disabled";
        }
        match self.backend {
            Backend::HighLevel => "DDC/CI",
            Backend::Vcp { .. } => "DDC/CI (VCP)",
            Backend::Wmi => "WMI",
            Backend::Unsupported => "Unsupported",
        }
    }

    /// The slider value as a whole percentage.
    pub fn percent(&self) -> u32 {
        (self.ui_value.clamp(0.0, 1.0) * 100.0).round() as u32
    }

    /// Sends the slider value to the worker, unless it is the value last sent.
    pub fn request_write(&mut self) {
        let percent = self.percent();
        if !self.accepts_input() || self.last_requested == Some(percent) {
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

    /// Starts a worker for every monitor, reporting to `hwnd`. Each worker probes its
    /// monitor, applies Smart Restore and reports `WM_APP_MONITOR_READY`.
    pub fn start_workers(&mut self, hwnd: HWND) {
        self.worker_generation = self.worker_generation.wrapping_add(1);
        let generation = self.worker_generation;
        for (index, m) in self.monitors.iter_mut().enumerate() {
            // Smart Restore: only reapply values this app set and confirmed.
            let restore = m
                .identity
                .as_ref()
                .and_then(|ident| self.settings.get_monitor_state(ident))
                .filter(|saved| brightness::should_restore(saved))
                .map(|saved| saved.brightness_value);
            let job = WorkerJob { physical: m.physical.take(), device_path: m.device_path.clone(), restore };
            m.worker = Worker::spawn(job, hwnd, generation, index);
            if m.worker.is_none() {
                // No thread, so no report will come: show the monitor as uncontrollable.
                m.ready = true;
            }
        }
    }

    /// Applies the worker's probe (and Smart Restore) result for monitor `idx`. Results
    /// from an older worker generation are ignored. Returns true if the monitor changed.
    pub fn handle_monitor_ready(&mut self, generation: u16, idx: usize, ready: MonitorReady) -> bool {
        if generation != self.worker_generation {
            return false;
        }
        let Some(m) = self.monitors.get_mut(idx) else {
            return false;
        };
        let probe = ready.probe;
        m.backend = probe.backend;
        m.hardware_min = probe.hardware_min;
        m.hardware_max = probe.hardware_max;

        let mut value = probe.value;
        if let Some(result) = ready.restore {
            m.apply_write_result(result);
            if result.success {
                value = Some(result.value);
                if m.identity.is_some() {
                    self.settings_dirty = true;
                }
            }
        }
        if let Some(value) = value {
            m.ui_value = value as f32 / 100.0;
        }
        m.last_requested = value;
        m.ready = true;
        true
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
        for m in self.monitors.iter_mut().filter(|m| m.accepts_input()) {
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
            if let Some(m) = self.monitors.get_mut(idx).filter(|m| m.accepts_input()) {
                m.ui_value = (m.ui_value + delta).clamp(0.0, 1.0);
            }
        }
    }

    pub fn set_absolute(&mut self, indices: &[usize], value: f32) {
        let value = value.clamp(0.0, 1.0);
        for &idx in indices {
            if let Some(m) = self.monitors.get_mut(idx).filter(|m| m.accepts_input()) {
                m.ui_value = value;
            }
        }
    }

    pub fn cycle_presets(&mut self, indices: &[usize]) {
        let presets = &self.settings.presets;
        if presets.is_empty() { return; }

        for &idx in indices {
            if let Some(m) = self.monitors.get_mut(idx).filter(|m| m.accepts_input()) {
                // Next preset strictly greater than the current value, wrapping to the first.
                m.ui_value = presets
                    .iter()
                    .copied()
                    .find(|&p| p > m.ui_value + 0.001)
                    .unwrap_or(presets[0]);
            }
        }
    }

    /// Turns the percentage label of monitor `idx` into an edit field. Any other edit in
    /// progress is committed first; clicking the label already being edited does nothing.
    pub fn begin_edit(&mut self, idx: usize) {
        if self.editing_monitor == Some(idx) {
            return;
        }
        self.commit_edit();
        if let Some(m) = self.monitors.get(idx).filter(|m| m.accepts_input()) {
            self.edit_buffer = m.percent().to_string();
            self.editing_monitor = Some(idx);
        }
    }

    /// Handles a typed character: digits (up to 3) and backspace.
    /// Returns true if the edit field changed.
    pub fn edit_input(&mut self, ch: char) -> bool {
        if self.editing_monitor.is_none() {
            return false;
        }
        match ch {
            '0'..='9' if self.edit_buffer.len() < MAX_EDIT_DIGITS => {
                self.edit_buffer.push(ch);
                true
            }
            '\u{8}' => self.edit_buffer.pop().is_some(),
            _ => false,
        }
    }

    /// Applies the edited value with one write (invalid text keeps the old value) and
    /// closes the edit field. Returns true if an edit was open.
    pub fn commit_edit(&mut self) -> bool {
        let Some(idx) = self.editing_monitor.take() else {
            return false;
        };
        if let Some(value) = parse_percent(&self.edit_buffer) {
            self.set_absolute(&[idx], value);
            self.request_brightness(&[idx]);
        }
        self.edit_buffer.clear();
        true
    }

    /// Closes the edit field without applying it. Returns true if an edit was open.
    pub fn cancel_edit(&mut self) -> bool {
        self.edit_buffer.clear();
        self.editing_monitor.take().is_some()
    }
}

/// The longest number the edit field accepts ("100").
const MAX_EDIT_DIGITS: usize = 3;

/// Parses typed percentage text (digits only) into a slider value, clamping
/// anything above 100 to 1.0.
pub fn parse_percent(text: &str) -> Option<f32> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // Only digits are left, so parsing can only fail by overflowing: that's above 100 too.
    let percent = text.parse::<u32>().unwrap_or(u32::MAX);
    Some(percent.min(100) as f32 / 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::Mailbox;
    use std::sync::Arc;

    fn monitor(value: f32) -> Monitor {
        let mut m = Monitor::new(None, 0, "Test".into(), None, String::new());
        m.backend = Backend::HighLevel;
        m.ready = true;
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

    fn probe(backend: Backend, value: Option<u32>) -> crate::brightness::Probe {
        crate::brightness::Probe { backend, hardware_min: 0, hardware_max: 100, value }
    }

    #[test]
    fn test_monitor_ignores_input_until_ready() {
        let mut state = state_with(&[0.5]);
        state.monitors[0].ready = false;
        let mailbox = Arc::new(Mailbox::default());
        state.monitors[0].worker = Some(Worker::detached(Arc::clone(&mailbox)));

        state.apply_step(&[0], 0.2);
        state.set_absolute(&[0], 0.9);
        state.settings.presets = vec![1.0];
        state.cycle_presets(&[0]);
        state.apply_sync_delta(0, 0.3);
        state.request_brightness(&[0]);
        assert_eq!(values(&state), vec![0.5]);
        assert_eq!(mailbox.take_pending(), None);
    }

    #[test]
    fn test_monitor_ready_applies_probe() {
        let mut state = state_with(&[0.5]);
        state.monitors[0].ready = false;
        state.monitors[0].backend = Backend::Unsupported;
        state.worker_generation = 2;

        let ready = MonitorReady { probe: probe(Backend::Vcp { max: 255 }, Some(30)), restore: None };
        assert!(!state.handle_monitor_ready(1, 0, ready.clone()), "stale generation ignored");
        assert!(!state.monitors[0].ready);

        assert!(state.handle_monitor_ready(2, 0, ready));
        let m = &state.monitors[0];
        assert!(m.ready && m.accepts_input());
        assert_eq!(m.backend, Backend::Vcp { max: 255 });
        assert_eq!(m.percent(), 30);
        assert_eq!(m.last_requested, Some(30), "the hardware already has this value");
        assert!(!state.settings_dirty);
    }

    #[test]
    fn test_monitor_ready_with_restore() {
        let mut state = AppState {
            monitors: vec![identified(0.5)],
            ..Default::default()
        };
        state.monitors[0].ready = false;
        let restored = WriteResult { value: 70, success: true, confirmed: Some(true) };

        state.handle_monitor_ready(0, 0, MonitorReady { probe: probe(Backend::HighLevel, Some(20)), restore: Some(restored) });
        let m = &state.monitors[0];
        assert_eq!(m.percent(), 70);
        assert_eq!(m.written_value, Some(70));
        assert!(m.write_confirmed);
        assert!(state.settings_dirty);

        // A failed restore leaves the probed value.
        let mut state = state_with(&[0.5]);
        state.monitors[0].ready = false;
        let failed = WriteResult { value: 70, success: false, confirmed: None };
        state.handle_monitor_ready(0, 0, MonitorReady { probe: probe(Backend::HighLevel, Some(20)), restore: Some(failed) });
        assert_eq!(state.monitors[0].percent(), 20);
        assert_eq!(state.monitors[0].failure_count, 1);
    }

    #[test]
    fn test_unsupported_monitor_ignores_input() {
        let mut state = state_with(&[0.5]);
        state.handle_monitor_ready(0, 0, MonitorReady { probe: probe(Backend::Unsupported, None), restore: None });
        assert!(state.monitors[0].ready);
        assert!(!state.monitors[0].accepts_input());
        state.apply_step(&[0], 0.1);
        assert_eq!(values(&state), vec![0.5]);
    }

    #[test]
    fn test_status_label() {
        let with_backend = |backend| {
            let mut m = monitor(0.5);
            m.backend = backend;
            m.status_label()
        };
        assert_eq!(with_backend(Backend::HighLevel), "DDC/CI");
        assert_eq!(with_backend(Backend::Vcp { max: 255 }), "DDC/CI (VCP)");
        assert_eq!(with_backend(Backend::Wmi), "WMI");
        assert_eq!(with_backend(Backend::Unsupported), "Unsupported");

        // A confirmed write doesn't change how the monitor is controlled.
        let mut m = monitor(0.5);
        m.write_confirmed = true;
        assert_eq!(m.status_label(), "DDC/CI");

        m.is_disabled = true;
        assert_eq!(m.status_label(), "Disabled");

        m.ready = false;
        assert_eq!(m.status_label(), "Detecting\u{2026}", "detecting wins over everything");
    }

    #[test]
    fn test_parse_percent() {
        assert_eq!(parse_percent(""), None);
        assert_eq!(parse_percent("0"), Some(0.0));
        assert_eq!(parse_percent("007"), Some(0.07));
        assert_eq!(parse_percent("100"), Some(1.0));
        assert_eq!(parse_percent("150"), Some(1.0));
        assert_eq!(parse_percent("99999999999"), Some(1.0));
        assert_eq!(parse_percent("abc"), None);
        assert_eq!(parse_percent("+5"), None);
        assert_eq!(parse_percent("-5"), None);
        assert_eq!(parse_percent("\u{0663}"), None, "non-ASCII digits are rejected");
    }

    /// A state with workers whose mailboxes the test can inspect.
    fn editable(values: &[f32]) -> (AppState, Vec<Arc<Mailbox>>) {
        let mut state = state_with(values);
        let mailboxes: Vec<_> = state
            .monitors
            .iter_mut()
            .map(|m| {
                let mailbox = Arc::new(Mailbox::default());
                m.worker = Some(Worker::detached(Arc::clone(&mailbox)));
                m.last_requested = Some(m.percent());
                mailbox
            })
            .collect();
        (state, mailboxes)
    }

    fn type_text(state: &mut AppState, text: &str) {
        for ch in text.chars() {
            state.edit_input(ch);
        }
    }

    #[test]
    fn test_edit_commit_writes_once() {
        let (mut state, mailboxes) = editable(&[0.5]);
        state.begin_edit(0);
        assert_eq!(state.edit_buffer, "50");

        type_text(&mut state, "\u{8}\u{8}3");
        assert_eq!(mailboxes[0].take_pending(), None, "nothing is written while typing");
        type_text(&mut state, "0");
        assert_eq!(state.edit_buffer, "30");

        assert!(state.commit_edit());
        assert_eq!(state.editing_monitor, None);
        assert_eq!(values(&state), vec![0.3]);
        assert_eq!(mailboxes[0].take_pending(), Some(30));
        assert!(!state.commit_edit(), "nothing left to commit");
    }

    #[test]
    fn test_edit_cancel_and_invalid_input_keep_value() {
        let (mut state, mailboxes) = editable(&[0.5]);
        state.begin_edit(0);
        type_text(&mut state, "\u{8}\u{8}9");
        assert!(state.cancel_edit());
        assert_eq!(values(&state), vec![0.5]);

        // An emptied field reverts too.
        state.begin_edit(0);
        type_text(&mut state, "\u{8}\u{8}\u{8}");
        state.commit_edit();
        assert_eq!(values(&state), vec![0.5]);

        // Letters never reach the buffer, and it holds at most 3 digits.
        state.begin_edit(0);
        assert!(!state.edit_input('a'));
        assert!(!state.edit_input('\u{0663}'));
        assert_eq!(state.edit_buffer, "50");
        assert!(state.edit_input('0'));
        assert!(!state.edit_input('1'));
        assert_eq!(state.edit_buffer, "500");
        state.cancel_edit();
        assert_eq!(mailboxes[0].take_pending(), None, "nothing was committed");

        // Committing the unchanged value writes nothing.
        state.begin_edit(0);
        state.commit_edit();
        assert_eq!(mailboxes[0].take_pending(), None, "unchanged value isn't written");
    }

    #[test]
    fn test_editing_another_label_commits_the_first() {
        let (mut state, mailboxes) = editable(&[0.5, 0.6]);
        state.begin_edit(0);
        type_text(&mut state, "\u{8}\u{8}20");

        // Clicking the same label keeps editing.
        state.begin_edit(0);
        assert_eq!(state.edit_buffer, "20");

        state.begin_edit(1);
        assert_eq!(state.editing_monitor, Some(1));
        assert_eq!(state.edit_buffer, "60");
        assert_eq!(values(&state), vec![0.2, 0.6]);
        assert_eq!(mailboxes[0].take_pending(), Some(20));
    }

    #[test]
    fn test_edit_requires_controllable_monitor() {
        let (mut state, _) = editable(&[0.5]);
        state.monitors[0].ready = false;
        state.begin_edit(0);
        assert_eq!(state.editing_monitor, None);
        assert!(!state.edit_input('1'));
        state.begin_edit(3);
        assert_eq!(state.editing_monitor, None);
    }
}
