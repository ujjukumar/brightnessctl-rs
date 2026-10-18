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

    #[test]
    fn test_unregister_all_releases_the_keys() {
        // Ctrl+Alt+Shift+Win+F24 on the test thread: nothing else holds it.
        let hotkeys = HashMap::from([(Action::StepUp, Hotkey { vkey: 0x87, modifiers: 0x000F })]);
        let thread = HWND::default();

        let mut first = HotkeyManager::new();
        first.register_all(thread, &hotkeys);
        assert_eq!(first.registrations.get(&Action::StepUp), Some(&true));

        let mut second = HotkeyManager::new();
        second.register_all(thread, &hotkeys);
        assert_eq!(second.registrations.get(&Action::StepUp), Some(&false), "key is still held");

        first.unregister_all(thread);
        assert!(first.registrations.is_empty());

        let mut third = HotkeyManager::new();
        third.register_all(thread, &hotkeys);
        assert_eq!(third.registrations.get(&Action::StepUp), Some(&true), "key was released");
        third.unregister_all(thread);
    }
}
