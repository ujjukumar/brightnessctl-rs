use serde::{Deserialize, Serialize};
use std::fs;
use windows::Win32::System::Registry::*;
use windows::core::w;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum ThemeMode {
    Light,
    Dark,
    Auto,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub theme: ThemeMode,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::Auto,
        }
    }
}

impl Settings {
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

    #[test]
    fn test_settings_serialization() {
        let settings = Settings {
            theme: ThemeMode::Dark,
        };
        let serialized = serde_json::to_string(&settings).unwrap();
        let deserialized: Settings = serde_json::from_str(&serialized).unwrap();
        assert_eq!(settings.theme, deserialized.theme);
    }

    #[test]
    fn test_settings_default() {
        let settings = Settings::default();
        assert_eq!(settings.theme, ThemeMode::Auto);
    }
}
