use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS};
use windows::Win32::Foundation::HWND;
use std::collections::HashMap;
use crate::actions::Action;
use crate::settings::Hotkey;

pub struct HotkeyManager {
    registrations: HashMap<Action, bool>, // Action -> IsRegistered
}

impl HotkeyManager {
    pub fn new() -> Self {
        Self {
            registrations: HashMap::new(),
        }
    }

    pub fn register_all(&mut self, hwnd: HWND, hotkeys: &HashMap<Action, Hotkey>) {
        for (&action, hotkey) in hotkeys {
            let id = action as i32;
            unsafe {
                let success = RegisterHotKey(
                    Some(hwnd),
                    id,
                    HOT_KEY_MODIFIERS(hotkey.modifiers),
                    hotkey.vkey,
                ).is_ok();
                
                self.registrations.insert(action, success);
            }
        }
    }

    pub fn unregister_all(&mut self, hwnd: HWND) {
        for (&action, &registered) in &self.registrations {
            if registered {
                let id = action as i32;
                unsafe {
                    let _ = UnregisterHotKey(Some(hwnd), id);
                }
            }
        }
        self.registrations.clear();
    }

    pub fn is_registered(&self, action: Action) -> bool {
        *self.registrations.get(&action).unwrap_or(&false)
    }

    pub fn sync_status(&self, state: &mut crate::state::AppState) {
        state.hotkey_status = self.registrations.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hotkey_manager_new() {
        let hkm = HotkeyManager::new();
        assert!(hkm.registrations.is_empty());
    }

    #[test]
    fn test_is_registered_fallback() {
        let hkm = HotkeyManager::new();
        assert!(!hkm.is_registered(Action::StepUp));
    }
}

impl Drop for HotkeyManager {
    fn drop(&mut self) {
        // Warning: Requires HWND to unregister, but Drop doesn't have it.
        // HWND is managed by the message loop, we should unregister before destruction.
    }
}
