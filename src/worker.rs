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
use crate::brightness::{self, BrightnessBackend, PhysicalHandle, Probe};
use crate::log::debug_log;

/// Posted to the window after each write: `wParam` is `pack_target`, `lParam` is
/// `WriteResult::to_lparam`.
pub const WM_APP_WRITE_DONE: u32 = WM_APP + 1;

/// Posted once per worker after probing: `wParam` is `pack_target`, `lParam` owns a
/// boxed `MonitorReady` (take it with `take_ready`).
pub const WM_APP_MONITOR_READY: u32 = WM_APP + 2;

/// Minimum time between the end of one hardware transaction (read or write) and the
/// start of the next on the same monitor.
pub const MIN_IO_GAP: Duration = Duration::from_millis(50);

/// What a worker needs to find its monitor.
#[derive(Debug)]
pub struct WorkerJob {
    pub physical: Option<PhysicalHandle>,
    pub device_path: String,
    /// Smart Restore value to apply once the backend is known, in percent.
    pub restore: Option<u32>,
}

/// A worker's report after probing (and restoring) its monitor.
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorReady {
    pub probe: Probe,
    /// The Smart Restore write, if one was needed.
    pub restore: Option<WriteResult>,
}

/// Takes ownership of the `MonitorReady` sent with `WM_APP_MONITOR_READY`.
///
/// # Safety
/// `lparam` must come from that message and be taken exactly once.
pub unsafe fn take_ready(lparam: LPARAM) -> Box<MonitorReady> {
    unsafe { Box::from_raw(lparam.0 as *mut MonitorReady) }
}

fn post_ready(hwnd: HWND, target: WPARAM, ready: MonitorReady) {
    let ready = Box::into_raw(Box::new(ready));
    unsafe {
        if PostMessageW(Some(hwnd), WM_APP_MONITOR_READY, target, LPARAM(ready as isize)).is_err() {
            // Never delivered, so nobody else will free it.
            drop(Box::from_raw(ready));
        }
    }
}

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

/// Spaces hardware transactions: each starts at least `gap` after the previous one
/// ended. DDC/CI monitors reject a command that arrives right after another (a write
/// issued straight after the probe read fails).
pub struct Pacer {
    gap: Duration,
    last: Option<Instant>,
}

impl Pacer {
    pub fn new(gap: Duration) -> Self {
        Self { gap, last: None }
    }

    /// Sleeps until the gap since the last transaction has passed. Returns true if it slept.
    fn wait(&mut self) -> bool {
        match self.last.map(|t| t.elapsed()) {
            Some(elapsed) if elapsed < self.gap => {
                thread::sleep(self.gap - elapsed);
                true
            }
            _ => false,
        }
    }

    /// Records that a transaction just ended.
    pub fn done(&mut self) {
        self.last = Some(Instant::now());
    }

    fn run<T>(&mut self, transaction: impl FnOnce() -> T) -> T {
        self.wait();
        let result = transaction();
        self.done();
        result
    }
}

/// Runs on a worker thread once its monitor is probed: applies Smart Restore, reports
/// `ready`, then writes requested values until the mailbox is stopped.
pub fn serve(
    probe: Probe,
    backend: &mut dyn BrightnessBackend,
    restore: Option<u32>,
    mailbox: &Mailbox,
    gap: Duration,
    ready: impl FnOnce(MonitorReady),
    report: impl FnMut(WriteResult),
) {
    let mut pacer = Pacer::new(gap);
    // Probing just talked to the monitor.
    pacer.done();

    let restore = restore.filter(|&saved| probe.value != Some(saved)).map(|saved| {
        let success = pacer.run(|| backend.write(saved));
        let confirmed = success.then(|| pacer.run(|| backend.verify(saved)));
        WriteResult { value: saved, success, confirmed }
    });
    ready(MonitorReady { probe, restore });

    run(backend, mailbox, &mut pacer, report);
}

/// Writes the newest value from `mailbox` to `backend` until the mailbox is stopped.
pub fn run(
    backend: &mut dyn BrightnessBackend,
    mailbox: &Mailbox,
    pacer: &mut Pacer,
    mut report: impl FnMut(WriteResult),
) {
    while let Some(mut value) = mailbox.wait() {
        if pacer.wait() {
            match mailbox.latest(value) {
                Some(latest) => value = latest,
                None => break,
            }
        }

        let write_started = Instant::now();
        let success = backend.write(value);
        pacer.done();

        // Only the value that ends a burst is read back; earlier ones are about to be replaced.
        let mut confirmed = None;
        if success && !mailbox.has_pending() {
            pacer.wait();
            if !mailbox.has_pending() {
                confirmed = Some(backend.verify(value));
                pacer.done();
            }
        }
        debug_log!("wrote {value}% (success {success}, confirmed {confirmed:?}) in {:?}", write_started.elapsed());
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
    /// Starts a worker for the monitor at `index`, reporting to `hwnd`. The worker first
    /// probes the monitor and applies `job.restore`, posts `WM_APP_MONITOR_READY`, then
    /// writes requested values until dropped.
    pub fn spawn(job: WorkerJob, hwnd: HWND, generation: u16, index: usize) -> Option<Self> {
        let mailbox = Arc::new(Mailbox::default());
        let thread_mailbox = Arc::clone(&mailbox);
        // HWND isn't Send, but posting to a window is allowed from any thread.
        let hwnd = hwnd.0 as isize;
        let thread = thread::Builder::new()
            .name(format!("monitor-{index}"))
            .spawn(move || {
                let hwnd = HWND(hwnd as *mut c_void);
                let target = WPARAM(pack_target(generation, index));
                let started = Instant::now();
                brightness::with_probed_backend(job.physical, &job.device_path, |probe, backend| {
                    debug_log!("probed {} as {:?} in {:?}", job.device_path, probe, started.elapsed());
                    let Some(backend) = backend else {
                        // Nothing can control this monitor; report it and end the thread.
                        post_ready(hwnd, target, MonitorReady { probe, restore: None });
                        return;
                    };
                    serve(
                        probe,
                        backend,
                        job.restore,
                        &thread_mailbox,
                        MIN_IO_GAP,
                        |ready| {
                            debug_log!("ready after {:?}, restore {:?}", started.elapsed(), ready.restore);
                            post_ready(hwnd, target, ready);
                        },
                        |result| unsafe {
                            let _ = PostMessageW(Some(hwnd), WM_APP_WRITE_DONE, target, LPARAM(result.to_lparam()));
                        },
                    );
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
        verifying: Sender<u32>,
        release: Option<Receiver<()>>,
        fail: bool,
        writes: Vec<(u32, Instant)>,
        verified: Vec<(u32, Instant)>,
    }

    impl FakeBackend {
        fn new(started: Sender<u32>, verifying: Sender<u32>, release: Option<Receiver<()>>, fail: bool) -> Self {
            Self { started, verifying, release, fail, writes: Vec::new(), verified: Vec::new() }
        }

        fn written(&self) -> Vec<u32> {
            self.writes.iter().map(|&(v, _)| v).collect()
        }
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
            self.verified.push((percent, Instant::now()));
            let _ = self.verifying.send(percent);
            self.read() == Some(percent)
        }
    }

    struct Harness {
        mailbox: Arc<Mailbox>,
        started: Receiver<u32>,
        verifying: Receiver<u32>,
        release: Sender<()>,
        thread: JoinHandle<(FakeBackend, Vec<WriteResult>)>,
    }

    fn start(blocking: bool, fail: bool, gap: Duration) -> Harness {
        let (started_tx, started) = channel();
        let (verifying_tx, verifying) = channel();
        let (release, release_rx) = channel();
        let mut fake = FakeBackend::new(started_tx, verifying_tx, blocking.then_some(release_rx), fail);
        let mailbox = Arc::new(Mailbox::default());
        let thread_mailbox = Arc::clone(&mailbox);
        let thread = thread::spawn(move || {
            let mut results = Vec::new();
            run(&mut fake, &thread_mailbox, &mut Pacer::new(gap), |r| results.push(r));
            (fake, results)
        });
        Harness { mailbox, started, verifying, release, thread }
    }

    impl Harness {
        fn finish(self) -> (FakeBackend, Vec<WriteResult>) {
            self.mailbox.stop();
            let _ = self.release.send(());
            self.thread.join().unwrap()
        }
    }

    struct ServeHarness {
        mailbox: Arc<Mailbox>,
        ready: Receiver<MonitorReady>,
        started: Receiver<u32>,
        probed_at: Instant,
        thread: JoinHandle<(FakeBackend, Vec<WriteResult>)>,
    }

    fn start_serving(probed_value: Option<u32>, restore: Option<u32>, gap: Duration) -> ServeHarness {
        let (started_tx, started) = channel();
        let (ready_tx, ready) = channel();
        let (verifying_tx, _) = channel();
        let mut fake = FakeBackend::new(started_tx, verifying_tx, None, false);
        let mailbox = Arc::new(Mailbox::default());
        let thread_mailbox = Arc::clone(&mailbox);
        let probe = Probe { backend: crate::state::Backend::HighLevel, hardware_min: 0, hardware_max: 100, value: probed_value };
        let probed_at = Instant::now();
        let thread = thread::spawn(move || {
            let mut results = Vec::new();
            serve(probe, &mut fake, restore, &thread_mailbox, gap, |r| ready_tx.send(r).unwrap(), |r| results.push(r));
            (fake, results)
        });
        ServeHarness { mailbox, ready, started, probed_at, thread }
    }

    impl ServeHarness {
        fn finish(self) -> (FakeBackend, Vec<WriteResult>) {
            self.mailbox.stop();
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
        assert_eq!(fake.written(), vec![1, 10]);
        assert_eq!(
            results,
            vec![
                WriteResult { value: 1, success: true, confirmed: None },
                WriteResult { value: 10, success: true, confirmed: Some(true) },
            ]
        );
        let verified: Vec<u32> = fake.verified.iter().map(|&(v, _)| v).collect();
        assert_eq!(verified, vec![10]);
    }

    #[test]
    fn test_minimum_gap_between_transactions() {
        let gap = Duration::from_millis(30);
        let h = start(false, false, gap);
        h.mailbox.post(1);
        assert_eq!(h.started.recv().unwrap(), 1);
        assert_eq!(h.verifying.recv().unwrap(), 1);
        h.mailbox.post(2);
        assert_eq!(h.started.recv().unwrap(), 2);

        let (fake, _) = h.finish();
        // write 1 -> read-back 1 -> write 2, each at least `gap` apart.
        let (first_write, verify, second_write) = (fake.writes[0].1, fake.verified[0].1, fake.writes[1].1);
        assert!(verify - first_write >= gap, "read-back only {:?} after the write", verify - first_write);
        assert!(second_write - verify >= gap, "write only {:?} after the read-back", second_write - verify);
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
    fn test_first_write_waits_after_probe() {
        let gap = Duration::from_millis(30);
        let h = start_serving(Some(10), None, gap);
        assert_eq!(h.ready.recv().unwrap().restore, None);
        h.mailbox.post(20);
        assert_eq!(h.started.recv().unwrap(), 20);
        let probed_at = h.probed_at;

        let (fake, _) = h.finish();
        let after_probe = fake.writes[0].1 - probed_at;
        assert!(after_probe >= gap, "write only {after_probe:?} after the probe");
    }

    #[test]
    fn test_restore_is_written_before_ready() {
        let h = start_serving(Some(20), Some(70), Duration::from_millis(5));
        let ready = h.ready.recv().unwrap();
        assert_eq!(ready.restore, Some(WriteResult { value: 70, success: true, confirmed: Some(true) }));
        assert_eq!(ready.probe.value, Some(20), "the probe is reported as read");

        let (fake, results) = h.finish();
        assert_eq!(fake.written(), vec![70]);
        assert!(results.is_empty(), "the restore isn't reported as a separate write");
    }

    #[test]
    fn test_restore_skipped_when_already_set() {
        let h = start_serving(Some(70), Some(70), Duration::from_millis(5));
        assert_eq!(h.ready.recv().unwrap().restore, None);
        let (fake, _) = h.finish();
        assert!(fake.writes.is_empty());
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
        for mut m in crate::monitors::enumerate_monitors() {
            let physical = m.physical.take();
            let device_path = m.device_path.clone();
            let mailbox = Arc::new(Mailbox::default());
            let thread_mailbox = Arc::clone(&mailbox);
            let (ready_tx, ready_rx) = channel();
            let (result_tx, result_rx) = channel();
            let thread = thread::spawn(move || {
                brightness::with_probed_backend(physical, &device_path, |probe, backend| {
                    let backend = backend.expect("usable backend");
                    serve(probe, backend, None, &thread_mailbox, MIN_IO_GAP, |r| ready_tx.send(r).unwrap(), |r| {
                        let _ = result_tx.send(r);
                    });
                })
            });

            // Write straight after the probe: the case DDC/CI monitors reject without pacing.
            let ready = ready_rx.recv_timeout(Duration::from_secs(5)).expect("probe");
            let value = ready.probe.value.expect("readable monitor");
            let started = Instant::now();
            mailbox.post(value);
            let result = result_rx.recv_timeout(Duration::from_secs(5)).expect("worker result");
            println!("{} ({:?}) -> {:?} in {:?}", m.device_path, ready.probe.backend, result, started.elapsed());
            mailbox.stop();
            thread.join().unwrap();
            assert_eq!(result, WriteResult { value, success: true, confirmed: Some(true) });
        }
    }
}
