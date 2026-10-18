//! One copy of the app per Windows session. A second launch brings the running
//! window forward and exits.

use windows::{
    core::{w, PCWSTR},
    Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE},
    Win32::System::Threading::CreateMutexW,
    Win32::UI::WindowsAndMessaging::{FindWindowW, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE},
};

use crate::window;

/// `Local\` makes it per session, so each signed-in user can run their own copy.
const MUTEX_NAME: PCWSTR = w!("Local\\brightnessctl.single-instance");

/// Holds the named mutex while this copy runs.
pub struct Instance(Option<HANDLE>);

impl Drop for Instance {
    fn drop(&mut self) {
        if let Some(handle) = self.0 {
            unsafe {
                let _ = CloseHandle(handle);
            }
        }
    }
}

/// `None` if another copy already runs in this session. If the mutex can't be
/// created at all, the app starts anyway.
pub fn acquire() -> Option<Instance> {
    unsafe {
        match CreateMutexW(None, false, MUTEX_NAME) {
            Ok(handle) if GetLastError() == ERROR_ALREADY_EXISTS => {
                let _ = CloseHandle(handle);
                None
            }
            Ok(handle) => Some(Instance(Some(handle))),
            Err(_) => Some(Instance(None)),
        }
    }
}

/// Restores and focuses the running copy's window. Does nothing if that copy is
/// still starting and has no window yet.
pub fn activate_existing() {
    unsafe {
        if let Ok(hwnd) = FindWindowW(window::CLASS_NAME, PCWSTR::null()) {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_second_acquire_fails_until_the_first_is_dropped() {
        // Skip if a real copy is running; it holds the mutex.
        let Some(first) = acquire() else { return };
        assert!(acquire().is_none());
        drop(first);
        assert!(acquire().is_some());
    }
}
