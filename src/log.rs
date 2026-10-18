//! Debug builds only: appends timing lines to `brightnessctl.log` next to the
//! executable. In release builds every call compiles to nothing.

/// Writes one line, tagged with the current thread's name.
pub fn write(args: std::fmt::Arguments) {
    #[cfg(debug_assertions)]
    {
        use std::io::Write;
        if let Some(path) = log_path()
            && let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path)
        {
            let thread = std::thread::current();
            let _ = writeln!(file, "[{}] {}", thread.name().unwrap_or("?"), args);
        }
    }
    #[cfg(not(debug_assertions))]
    let _ = args;
}

/// Starts a fresh log for this run.
pub fn reset() {
    #[cfg(debug_assertions)]
    if let Some(path) = log_path() {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(debug_assertions)]
fn log_path() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok().map(|exe| exe.with_file_name("brightnessctl.log"))
}

/// Logs a line in debug builds. Arguments are still type-checked in release builds,
/// so values computed only for logging don't trigger unused warnings.
macro_rules! debug_log {
    ($($arg:tt)*) => {
        if cfg!(debug_assertions) {
            $crate::log::write(format_args!($($arg)*));
        }
    };
}
pub(crate) use debug_log;
