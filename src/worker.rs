//! One background thread per monitor for all hardware I/O.
//!
//! The window thread never waits on a monitor. It posts the newest wanted value into
//! the worker's mailbox and returns. The worker writes only the newest value (values in
//! between are skipped, the last one is never lost), keeps a minimum gap between writes
//! so the DDC/CI bus isn't flooded, reads the final value of a burst back to confirm it,
//! and reports every write to the window with `WM_APP_WRITE_DONE`.

use std::ffi::c_void;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};
use crate::brightness::{self, BackendSpec, BrightnessBackend};

/// Posted to the window after each write: `wParam` is `pack_target`, `lParam` is
/// `WriteResult::to_lparam`.
pub const WM_APP_WRITE_DONE: u32 = WM_APP + 1;

/// Minimum time between two writes to the same monitor.
pub const MIN_WRITE_GAP: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteResult {
    /// Brightness written, in percent.
    pub value: u32,
    pub success: bool,
    /// Whether the monitor reported the value back. `None` when a newer value was
    /// already waiting, so this one wasn't checked.
    pub confirmed: Option<bool>,
}

impl WriteResult {
    pub fn to_lparam(self) -> isize {
        let mut bits = self.value.min(0xFF) as isize;
        if self.success {
            bits |= 1 << 8;
        }
        if let Some(confirmed) = self.confirmed {
            bits |= 1 << 9;
            if confirmed {
                bits |= 1 << 10;
            }
        }
        bits
    }

    pub fn from_lparam(lparam: isize) -> Self {
        Self {
            value: (lparam & 0xFF) as u32,
            success: lparam & (1 << 8) != 0,
            confirmed: (lparam & (1 << 9) != 0).then_some(lparam & (1 << 10) != 0),
        }
    }
}

/// Packs a monitor index with the worker generation. Results still queued from the
/// workers of a previous monitor list (before a refresh) carry an older generation.
pub fn pack_target(generation: u16, index: usize) -> usize {
    ((generation as usize) << 16) | (index & 0xFFFF)
}

pub fn unpack_target(wparam: usize) -> (u16, usize) {
    (((wparam >> 16) & 0xFFFF) as u16, wparam & 0xFFFF)
}

#[derive(Debug, Default)]
struct MailboxState {
    pending: Option<u32>,
    stop: bool,
}

/// Holds only the newest requested value.
#[derive(Debug, Default)]
pub struct Mailbox {
    state: Mutex<MailboxState>,
    ready: Condvar,
}

impl Mailbox {
    /// Replaces any waiting value with `value`.
    pub fn post(&self, value: u32) {
        self.lock().pending = Some(value);
        self.ready.notify_one();
    }

    pub fn stop(&self) {
        self.lock().stop = true;
        self.ready.notify_one();
    }

    /// Blocks until a value is waiting. `None` once stopped.
    fn wait(&self) -> Option<u32> {
        let mut state = self.lock();
        loop {
            if state.stop {
                return None;
            }
            if let Some(value) = state.pending.take() {
                return Some(value);
            }
            state = self.ready.wait(state).unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// The newest value if one arrived meanwhile, otherwise `current`. `None` once stopped.
    fn latest(&self, current: u32) -> Option<u32> {
        let mut state = self.lock();
        if state.stop {
            return None;
        }
        Some(state.pending.take().unwrap_or(current))
    }

    fn has_pending(&self) -> bool {
        self.lock().pending.is_some()
    }

    #[cfg(test)]
    pub fn take_pending(&self) -> Option<u32> {
        self.lock().pending.take()
    }

    fn lock(&self) -> MutexGuard<'_, MailboxState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Writes the newest value from `mailbox` to `backend` until the mailbox is stopped.
pub fn run(
    backend: &mut dyn BrightnessBackend,
    mailbox: &Mailbox,
    min_gap: Duration,
    mut report: impl FnMut(WriteResult),
) {
    let mut last_write: Option<Instant> = None;
    while let Some(mut value) = mailbox.wait() {
        if let Some(elapsed) = last_write.map(|t| t.elapsed())
            && elapsed < min_gap
        {
            thread::sleep(min_gap - elapsed);
            match mailbox.latest(value) {
                Some(latest) => value = latest,
                None => break,
            }
        }

        let success = backend.write(value);
        last_write = Some(Instant::now());

        // Only the value that ends a burst is read back; earlier ones are about to be replaced.
        let confirmed = (success && !mailbox.has_pending()).then(|| backend.verify(value));
        report(WriteResult { value, success, confirmed });
    }
}

/// A running worker. Dropping it stops the thread and waits for it to finish.
#[derive(Debug)]
pub struct Worker {
    mailbox: Arc<Mailbox>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    /// Starts a worker for the monitor at `index`, reporting results to `hwnd`.
    pub fn spawn(spec: BackendSpec, hwnd: HWND, generation: u16, index: usize) -> Option<Self> {
        let mailbox = Arc::new(Mailbox::default());
        let thread_mailbox = Arc::clone(&mailbox);
        // HWND isn't Send, but posting to a window is allowed from any thread.
        let hwnd = hwnd.0 as isize;
        let thread = thread::Builder::new()
            .name(format!("monitor-{index}"))
            .spawn(move || {
                let hwnd = HWND(hwnd as *mut c_void);
                let target = WPARAM(pack_target(generation, index));
                brightness::with_backend(spec, |backend| {
                    run(backend, &thread_mailbox, MIN_WRITE_GAP, |result| unsafe {
                        let _ = PostMessageW(Some(hwnd), WM_APP_WRITE_DONE, target, LPARAM(result.to_lparam()));
                    });
                });
            })
            .ok()?;
        Some(Self { mailbox, thread: Some(thread) })
    }

    /// Asks the worker to write `percent`, replacing any value still waiting.
    pub fn request(&self, percent: u32) {
        self.mailbox.post(percent);
    }

    /// A worker without a thread, for testing what gets posted to it.
    #[cfg(test)]
    pub fn detached(mailbox: Arc<Mailbox>) -> Self {
        Self { mailbox, thread: None }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.mailbox.stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{channel, Receiver, Sender};

    /// Records writes; optionally blocks inside `write` until released.
    struct FakeBackend {
        started: Sender<u32>,
        release: Option<Receiver<()>>,
        fail: bool,
        writes: Vec<(u32, Instant)>,
        verified: Vec<u32>,
    }

    impl BrightnessBackend for FakeBackend {
        fn read(&mut self) -> Option<u32> {
            self.writes.last().map(|&(value, _)| value)
        }

        fn write(&mut self, percent: u32) -> bool {
            self.writes.push((percent, Instant::now()));
            let _ = self.started.send(percent);
            if let Some(release) = &self.release {
                let _ = release.recv();
            }
            !self.fail
        }

        fn verify(&mut self, percent: u32) -> bool {
            self.verified.push(percent);
            self.read() == Some(percent)
        }
    }

    struct Harness {
        mailbox: Arc<Mailbox>,
        started: Receiver<u32>,
        release: Sender<()>,
        thread: JoinHandle<(FakeBackend, Vec<WriteResult>)>,
    }

    fn start(blocking: bool, fail: bool, gap: Duration) -> Harness {
        let (started_tx, started) = channel();
        let (release, release_rx) = channel();
        let mut fake = FakeBackend {
            started: started_tx,
            release: blocking.then_some(release_rx),
            fail,
            writes: Vec::new(),
            verified: Vec::new(),
        };
        let mailbox = Arc::new(Mailbox::default());
        let thread_mailbox = Arc::clone(&mailbox);
        let thread = thread::spawn(move || {
            let mut results = Vec::new();
            run(&mut fake, &thread_mailbox, gap, |r| results.push(r));
            (fake, results)
        });
        Harness { mailbox, started, release, thread }
    }

    impl Harness {
        fn finish(self) -> (FakeBackend, Vec<WriteResult>) {
            self.mailbox.stop();
            let _ = self.release.send(());
            self.thread.join().unwrap()
        }
    }

    #[test]
    fn test_burst_writes_only_first_and_last() {
        let h = start(true, false, Duration::from_millis(5));
        h.mailbox.post(1);
        assert_eq!(h.started.recv().unwrap(), 1);
        // While 1 is being written, a burst arrives; only its last value survives.
        for value in 2..=10 {
            h.mailbox.post(value);
        }
        h.release.send(()).unwrap();
        assert_eq!(h.started.recv().unwrap(), 10);
        h.release.send(()).unwrap();

        let (fake, results) = h.finish();
        let written: Vec<u32> = fake.writes.iter().map(|&(v, _)| v).collect();
        assert_eq!(written, vec![1, 10]);
        assert_eq!(
            results,
            vec![
                WriteResult { value: 1, success: true, confirmed: None },
                WriteResult { value: 10, success: true, confirmed: Some(true) },
            ]
        );
        assert_eq!(fake.verified, vec![10]);
    }

    #[test]
    fn test_minimum_gap_between_writes() {
        let gap = Duration::from_millis(30);
        let h = start(false, false, gap);
        h.mailbox.post(1);
        assert_eq!(h.started.recv().unwrap(), 1);
        h.mailbox.post(2);
        assert_eq!(h.started.recv().unwrap(), 2);

        let (fake, _) = h.finish();
        let elapsed = fake.writes[1].1 - fake.writes[0].1;
        assert!(elapsed >= gap, "writes only {elapsed:?} apart");
    }

    #[test]
    fn test_failed_write_is_reported_and_not_verified() {
        let h = start(false, true, Duration::from_millis(5));
        h.mailbox.post(5);
        assert_eq!(h.started.recv().unwrap(), 5);

        let (fake, results) = h.finish();
        assert_eq!(results, vec![WriteResult { value: 5, success: false, confirmed: None }]);
        assert!(fake.verified.is_empty());
    }

    #[test]
    fn test_mailbox_keeps_only_newest_value() {
        let mailbox = Mailbox::default();
        mailbox.post(1);
        mailbox.post(2);
        mailbox.post(3);
        assert_eq!(mailbox.wait(), Some(3));
        mailbox.post(4);
        mailbox.stop();
        assert_eq!(mailbox.wait(), None);
        assert_eq!(mailbox.latest(7), None);
    }

    #[test]
    fn test_write_result_round_trip() {
        for value in [0, 1, 50, 99, 100] {
            for success in [false, true] {
                for confirmed in [None, Some(false), Some(true)] {
                    let result = WriteResult { value, success, confirmed };
                    assert_eq!(WriteResult::from_lparam(result.to_lparam()), result);
                }
            }
        }
    }

    #[test]
    fn test_target_round_trip() {
        assert_eq!(unpack_target(pack_target(0, 0)), (0, 0));
        assert_eq!(unpack_target(pack_target(7, 3)), (7, 3));
        assert_eq!(unpack_target(pack_target(u16::MAX, 0xFFFF)), (u16::MAX, 0xFFFF));
    }

    #[test]
    #[ignore = "requires connected monitors; rewrites each monitor's current brightness"]
    fn test_worker_rewrites_current_value_on_hardware() {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let mut monitors = crate::monitors::enumerate_monitors();
        let current = brightness::probe_all(&mut monitors);
        for (m, value) in monitors.iter_mut().zip(current) {
            let value = value.expect("readable monitor");
            let spec = brightness::worker_spec(m).expect("usable backend");
            let mailbox = Arc::new(Mailbox::default());
            let thread_mailbox = Arc::clone(&mailbox);
            let (tx, rx) = channel();
            let thread = thread::spawn(move || {
                brightness::with_backend(spec, |backend| {
                    run(backend, &thread_mailbox, MIN_WRITE_GAP, |r| {
                        let _ = tx.send(r);
                    })
                })
            });

            let started = Instant::now();
            mailbox.post(value);
            let result = rx.recv_timeout(Duration::from_secs(5)).expect("worker result");
            println!("{} -> {:?} in {:?}", m.device_path, result, started.elapsed());
            mailbox.stop();
            thread.join().unwrap();
            assert_eq!(result, WriteResult { value, success: true, confirmed: Some(true) });
        }
    }
}
