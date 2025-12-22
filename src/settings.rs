use serde::{Deserialize, Serialize};
use std::fs;
use std::collections::HashMap;
use windows::Win32::System::Registry::*;
use windows::core::w;
use crate::state::MonitorIdentity;
use crate::actions::Action;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum ThemeMode {
    Light,
    Dark,
    Auto,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct MonitorState {
    pub brightness_value: u32,
    pub normalized_value: f32,
    pub last_set_by_app: bool,
    pub write_confirmed: bool,
    pub timestamp: u64,
    pub last_seen_device_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Hotkey {
    pub vkey: u32,
    pub modifiers: u32, // MOD_CONTROL | MOD_ALT etc
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub theme: ThemeMode,
    #[serde(default)]
    pub monitor_states: HashMap<String, MonitorState>,
    #[serde(default = "default_presets")]
    pub presets: Vec<f32>,
    #[serde(default = "default_hotkeys")]
    pub hotkeys: HashMap<Action, Hotkey>,
    #[serde(default = "default_step_size")]
    pub step_size: f32,
}

fn default_presets() -> Vec<f32> {
    vec![0.0, 0.25, 0.5, 0.75, 1.0]
}

fn default_step_size() -> f32 {
    0.1
}

fn default_hotkeys() -> HashMap<Action, Hotkey> {
    let mut map = HashMap::new();
    // Default bindings: Ctrl + Alt + Up/Down/P
    // MOD_ALT = 0x0001, MOD_CONTROL = 0x0002
    let modifiers = 0x0002 | 0x0001; 
    
    map.insert(Action::StepUp, Hotkey { vkey: 0x26, modifiers }); // VK_UP
    map.insert(Action::StepDown, Hotkey { vkey: 0x28, modifiers }); // VK_DOWN
    map.insert(Action::CyclePresets, Hotkey { vkey: 0x50, modifiers }); // 'P'
    
    map
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::Auto,
            monitor_states: HashMap::new(),
            presets: default_presets(),
            hotkeys: default_hotkeys(),
            step_size: default_step_size(),
        }
    }
}

impl Settings {
    pub fn get_monitor_state(&self, identity: &MonitorIdentity) -> Option<&MonitorState> {
        self.monitor_states.get(&identity.composite_key())
    }

    pub fn set_monitor_state(&mut self, identity: &MonitorIdentity, state: MonitorState) {
        self.monitor_states.insert(identity.composite_key(), state);
    }

    pub fn is_dark_mode(&self) -> bool {
        match self.theme {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            ThemeMode::Auto => Self::is_system_dark_mode(),
        }
    }

    pub fn is_system_dark_mode() -> bool {
        unsafe {
            let mut hkey = HKEY::default();
            if RegOpenKeyExW(
                HKEY_CURRENT_USER,
                w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
                None,
                KEY_READ,
                &mut hkey,
            ).is_ok() {
                let mut data = 0u32;
                let mut size = std::mem::size_of::<u32>() as u32;
                let mut kind = REG_VALUE_TYPE::default();
                if RegQueryValueExW(
                    hkey,
                    w!("AppsUseLightTheme"),
                    None,
                    Some(&mut kind),
                    Some(&mut data as *mut _ as *mut u8),
                    Some(&mut size),
                ).is_ok() {
                    let _ = RegCloseKey(hkey);
                    return data == 0; // 0 means Dark Mode
                }
                let _ = RegCloseKey(hkey);
            }
        }
        true // Default to dark if detection fails
    }

    pub fn load() -> Self {
        if let Ok(content) = fs::read_to_string("brightnessctl.json") {
            if let Ok(settings) = serde_json::from_str(&content) {
                return settings;
            }
        }
        let default = Self::default();
        let _ = default.save(); // Try to save default
        default
    }

    pub fn save(&self) -> std::io::Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write("brightnessctl.json", content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::MonitorIdentity;

    #[test]
    fn test_monitor_state_persistence() {
        let mut settings = Settings::default();
        let identity = MonitorIdentity {
            manufacturer_id: "DEL".to_string(),
            product_code: 123,
            serial: "SN1".to_string(),
            is_fallback: false,
        };
        let state = MonitorState {
            brightness_value: 75,
            normalized_value: 0.75,
            last_set_by_app: true,
            write_confirmed: true,
            timestamp: 123456789,
            last_seen_device_path: "path/to/device".to_string(),
        };
        
        settings.set_monitor_state(&identity, state.clone());
        let retrieved = settings.get_monitor_state(&identity).unwrap();
        assert_eq!(retrieved, &state);
    }

    #[test]
    fn test_multiple_monitor_persistence() {
        let mut settings = Settings::default();
        let id1 = MonitorIdentity { manufacturer_id: "A".into(), product_code: 1, serial: "S1".into(), is_fallback: false };
        let id2 = MonitorIdentity { manufacturer_id: "B".into(), product_code: 2, serial: "S2".into(), is_fallback: false };
        
        let st1 = MonitorState { brightness_value: 10, normalized_value: 0.1, last_set_by_app: true, write_confirmed: true, timestamp: 1, last_seen_device_path: "p1".into() };
        let st2 = MonitorState { brightness_value: 20, normalized_value: 0.2, last_set_by_app: false, write_confirmed: false, timestamp: 2, last_seen_device_path: "p2".into() };

        settings.set_monitor_state(&id1, st1.clone());
        settings.set_monitor_state(&id2, st2.clone());

        assert_eq!(settings.get_monitor_state(&id1).unwrap(), &st1);
        assert_eq!(settings.get_monitor_state(&id2).unwrap(), &st2);
    }

    #[test]
    fn test_settings_serialization() {
        let mut settings = Settings::default();
        settings.theme = ThemeMode::Dark;
        let serialized = serde_json::to_string(&settings).unwrap();
        let deserialized: Settings = serde_json::from_str(&serialized).unwrap();
        assert_eq!(settings.theme, deserialized.theme);
        assert_eq!(settings.presets, deserialized.presets);
        assert_eq!(settings.hotkeys.get(&Action::StepUp), deserialized.hotkeys.get(&Action::StepUp));
    }

    #[test]
    fn test_settings_default() {
        let settings = Settings::default();
        assert_eq!(settings.theme, ThemeMode::Auto);
        assert!(!settings.presets.is_empty());
        assert!(settings.hotkeys.contains_key(&Action::StepUp));
    }

        #[test]

        fn test_custom_hotkey_serialization() {

            let mut settings = Settings::default();

            let custom_hotkey = Hotkey { vkey: 0x41, modifiers: 0x0001 }; // Alt + A

            settings.hotkeys.insert(Action::StepUp, custom_hotkey.clone());

            

            let serialized = serde_json::to_string(&settings).unwrap();

            let deserialized: Settings = serde_json::from_str(&serialized).unwrap();

            

            assert_eq!(deserialized.hotkeys.get(&Action::StepUp).unwrap(), &custom_hotkey);

        }

    }

    