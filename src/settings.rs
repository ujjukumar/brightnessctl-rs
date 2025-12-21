use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum ThemeMode {
    Light,
    Dark,
    Auto,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub theme: ThemeMode,
    // Future expansion: window position, etc.
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::Auto,
        }
    }
}

impl Settings {
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
