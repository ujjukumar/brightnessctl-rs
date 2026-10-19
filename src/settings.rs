use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use windows::Win32::System::Registry::*;
use windows::core::w;
use crate::state::MonitorIdentity;
use crate::actions::Action;

const CONFIG_FILE_NAME: &str = "brightnessctl.json";
/// Folder under `%APPDATA%` that holds the config.
const CONFIG_DIR_NAME: &str = "brightnessctl";

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum ThemeMode {
    Light,
    Dark,
    Auto,
    /// Creamy white with a soft gradient and translucent panels over the background.
    Glass,
}

impl ThemeMode {
    /// Whether to draw dark; `Auto` follows Windows' app mode. `Glass` is a bright theme.
    pub fn is_dark(self, system_dark: bool) -> bool {
        match self {
            ThemeMode::Dark => true,
            ThemeMode::Light | ThemeMode::Glass => false,
            ThemeMode::Auto => system_dark,
        }
    }

    /// The themes in the order their buttons are laid out, left to right.
    pub const ALL: [ThemeMode; 4] =
        [ThemeMode::Auto, ThemeMode::Light, ThemeMode::Dark, ThemeMode::Glass];

    /// The name shown on this theme's button.
    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::Auto => "Auto",
            ThemeMode::Light => "Light",
            ThemeMode::Dark => "Dark",
            ThemeMode::Glass => "Glass",
        }
    }
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

/// Missing fields fall back to `Settings::default()`.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeMode,
    pub monitor_states: HashMap<String, MonitorState>,
    pub presets: Vec<f32>,
    pub hotkeys: HashMap<Action, Hotkey>,
    pub step_size: f32,
    /// Picture drawn behind the sliders; `None` keeps the plain background.
    pub background_image: Option<PathBuf>,
    /// How much the picture is dimmed toward the theme color: 0.0 shows it at full
    /// strength, 1.0 hides it.
    pub background_dim: f32,
}

fn default_presets() -> Vec<f32> {
    vec![0.0, 0.25, 0.5, 0.75, 1.0]
}

fn default_step_size() -> f32 {
    0.1
}

fn default_background_dim() -> f32 {
    0.6
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
            background_image: None,
            background_dim: default_background_dim(),
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

    /// Whether Windows apps are set to dark mode. This reads the registry, so callers
    /// keep the result and only read it again on `WM_SETTINGCHANGE`.
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

    /// `%APPDATA%\brightnessctl\brightnessctl.json`. Falls back to the executable's
    /// folder if `APPDATA` isn't set.
    pub fn default_path() -> PathBuf {
        let dir = match std::env::var_os("APPDATA") {
            Some(appdata) if !appdata.is_empty() => PathBuf::from(appdata).join(CONFIG_DIR_NAME),
            _ => exe_dir().unwrap_or_default(),
        };
        dir.join(CONFIG_FILE_NAME)
    }

    /// Loads the config from `default_path`, first moving one that an older version
    /// left next to the executable. See `load_from`.
    pub fn load() -> (Self, Option<String>) {
        let path = Self::default_path();
        let moved = exe_dir().is_some_and(|dir| move_legacy_config(&dir.join(CONFIG_FILE_NAME), &path));
        let (settings, warning) = Self::load_from(&path);
        let notice = moved.then(|| format!("Settings moved to %APPDATA%\\{CONFIG_DIR_NAME}"));
        (settings, warning.or(notice))
    }

    /// Loads settings from `path`, returning a status warning if the file had to be replaced.
    ///
    /// A missing file is created with defaults. A file that fails to parse is renamed to
    /// `<name>.bak` (replacing an older backup) and defaults are used; the user's file is
    /// never overwritten in place.
    pub fn load_from(path: &Path) -> (Self, Option<String>) {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                let settings = Self::default();
                let _ = settings.save_to(path);
                return (settings, None);
            }
            Err(err) => return (Self::default(), Some(format!("Config unreadable ({err}); using defaults"))),
        };

        // Some editors (and PowerShell 5) save UTF-8 with a byte order mark.
        let json = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
        match serde_json::from_slice::<Settings>(json) {
            Ok(mut settings) => {
                settings.sanitize();
                (settings, None)
            }
            Err(err) => {
                let location = format!("line {}, column {}", err.line(), err.column());
                let settings = Self::default();
                let warning = if fs::rename(path, backup_path(path)).is_ok() {
                    let _ = settings.save_to(path);
                    format!("Config error at {location}. Saved as .bak; using defaults")
                } else {
                    format!("Config error at {location}; using defaults")
                };
                (settings, Some(warning))
            }
        }
    }

    /// Saves to `default_path`.
    pub fn save(&self) -> io::Result<()> {
        self.save_to(&Self::default_path())
    }

    /// Writes a temporary file and renames it over `path`, so a crash mid-write
    /// can't leave a truncated config behind. Creates the folder if it's missing.
    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, content)?;
        fs::rename(&tmp, path)
    }

    /// Clamps values a hand-edited config could get wrong.
    fn sanitize(&mut self) {
        self.presets.retain(|p| p.is_finite());
        for preset in &mut self.presets {
            *preset = preset.clamp(0.0, 1.0);
        }
        if !(self.step_size.is_finite() && self.step_size > 0.0 && self.step_size <= 1.0) {
            self.step_size = default_step_size();
        }
        self.background_dim = if self.background_dim.is_finite() {
            self.background_dim.clamp(0.0, 1.0)
        } else {
            default_background_dim()
        };
    }
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(Path::to_path_buf)
}

/// Moves a config (and its `.bak`) that older versions kept next to the executable to
/// `path`. Does nothing once `path` exists. Returns whether the config was moved.
fn move_legacy_config(legacy: &Path, path: &Path) -> bool {
    if path.exists() || !legacy.is_file() {
        return false;
    }
    let moved = move_file(legacy, path).is_ok();
    if moved && backup_path(legacy).is_file() && !backup_path(path).exists() {
        let _ = move_file(&backup_path(legacy), &backup_path(path));
    }
    moved
}

/// Copies then deletes, since `fs::rename` can't move across drives. If the old file
/// can't be deleted (a read-only folder), the copy is still used.
fn move_file(from: &Path, to: &Path) -> io::Result<()> {
    if let Some(dir) = to.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::copy(from, to)?;
    let _ = fs::remove_file(from);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::MonitorIdentity;

    /// A fresh, empty directory for one test.
    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("brightnessctl-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_default_path_is_in_appdata() {
        let path = Settings::default_path();
        assert_eq!(path.file_name().unwrap(), CONFIG_FILE_NAME);
        if let Some(appdata) = std::env::var_os("APPDATA") {
            assert_eq!(path, PathBuf::from(appdata).join(CONFIG_DIR_NAME).join(CONFIG_FILE_NAME));
        }
    }

    #[test]
    fn test_save_creates_missing_folders() {
        let path = test_dir("folders").join("a").join("b").join(CONFIG_FILE_NAME);
        Settings::default().save_to(&path).unwrap();
        assert_eq!(Settings::load_from(&path), (Settings::default(), None));
    }

    #[test]
    fn test_legacy_config_and_backup_are_moved() {
        let dir = test_dir("legacy-move");
        let legacy = dir.join("exe").join(CONFIG_FILE_NAME);
        let path = dir.join("appdata").join(CONFIG_DIR_NAME).join(CONFIG_FILE_NAME);
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(&legacy, r#"{ "theme": "Light" }"#).unwrap();
        fs::write(backup_path(&legacy), "old backup").unwrap();

        assert!(move_legacy_config(&legacy, &path));
        assert!(!legacy.exists() && !backup_path(&legacy).exists());
        assert_eq!(fs::read_to_string(backup_path(&path)).unwrap(), "old backup");
        assert_eq!(Settings::load_from(&path).0.theme, ThemeMode::Light);
    }

    #[test]
    fn test_legacy_config_never_replaces_an_existing_one() {
        let dir = test_dir("legacy-keep");
        let legacy = dir.join("legacy.json");
        let path = dir.join(CONFIG_FILE_NAME);
        fs::write(&legacy, r#"{ "theme": "Light" }"#).unwrap();
        fs::write(&path, r#"{ "theme": "Dark" }"#).unwrap();

        assert!(!move_legacy_config(&legacy, &path));
        assert!(legacy.exists());
        assert_eq!(Settings::load_from(&path).0.theme, ThemeMode::Dark);

        // Nothing to move.
        assert!(!move_legacy_config(&dir.join("missing.json"), &dir.join("new.json")));
        assert!(!dir.join("new.json").exists());
    }

    #[test]
    fn test_missing_file_is_created_with_defaults() {
        let path = test_dir("missing").join(CONFIG_FILE_NAME);
        let (settings, warning) = Settings::load_from(&path);
        assert_eq!(settings, Settings::default());
        assert!(warning.is_none());
        assert!(path.exists());
    }

    #[test]
    fn test_missing_fields_use_defaults() {
        let path = test_dir("partial").join(CONFIG_FILE_NAME);

        fs::write(&path, r#"{ "step_size": 0.2 }"#).unwrap();
        let (settings, warning) = Settings::load_from(&path);
        assert!(warning.is_none());
        assert_eq!(settings.theme, ThemeMode::Auto);
        assert_eq!(settings.step_size, 0.2);
        assert_eq!(settings.presets, default_presets());
        assert_eq!(settings.hotkeys, default_hotkeys());

        fs::write(&path, "{}").unwrap();
        assert_eq!(Settings::load_from(&path).0, Settings::default());
    }

    #[test]
    fn test_utf8_byte_order_mark_is_accepted() {
        let path = test_dir("bom").join(CONFIG_FILE_NAME);

        fs::write(&path, b"\xEF\xBB\xBF{ \"theme\": \"Light\" }").unwrap();
        let (settings, warning) = Settings::load_from(&path);
        assert!(warning.is_none(), "{warning:?}");
        assert_eq!(settings.theme, ThemeMode::Light);
        assert!(!backup_path(&path).exists());
    }

    #[test]
    fn test_invalid_json_is_backed_up_not_overwritten() {
        let dir = test_dir("invalid");
        let path = dir.join(CONFIG_FILE_NAME);
        let broken = "{\n  \"theme\": \"Dark\",\n  oops\n}";
        fs::write(&path, broken).unwrap();
        fs::write(backup_path(&path), "older backup").unwrap();

        let (settings, warning) = Settings::load_from(&path);
        assert_eq!(settings, Settings::default());
        let warning = warning.unwrap();
        assert!(warning.contains("line 3"), "{warning}");
        assert!(warning.contains(".bak"), "{warning}");

        // The user's text is kept in the backup; the config itself is valid again.
        assert_eq!(fs::read_to_string(backup_path(&path)).unwrap(), broken);
        let (reloaded, warning) = Settings::load_from(&path);
        assert!(warning.is_none());
        assert_eq!(reloaded, Settings::default());
    }

    #[test]
    fn test_out_of_range_values_are_sanitized() {
        let path = test_dir("sanitize").join(CONFIG_FILE_NAME);
        fs::write(&path, r#"{ "presets": [-0.5, 0.3, 2.0], "step_size": 5.0 }"#).unwrap();
        let (settings, _) = Settings::load_from(&path);
        assert_eq!(settings.presets, vec![0.0, 0.3, 1.0]);
        assert_eq!(settings.step_size, default_step_size());

        for bad_step in ["0", "-0.1"] {
            fs::write(&path, format!(r#"{{ "step_size": {bad_step} }}"#)).unwrap();
            assert_eq!(Settings::load_from(&path).0.step_size, default_step_size());
        }

        for (dim, expected) in [("-0.5", 0.0), ("0.25", 0.25), ("3", 1.0)] {
            fs::write(&path, format!(r#"{{ "background_dim": {dim} }}"#)).unwrap();
            assert_eq!(Settings::load_from(&path).0.background_dim, expected);
        }
    }

    #[test]
    fn test_save_then_load_round_trip() {
        let path = test_dir("roundtrip").join(CONFIG_FILE_NAME);
        let mut settings = Settings {
            theme: ThemeMode::Light,
            step_size: 0.05,
            background_image: Some(PathBuf::from(r"C:\Users\Example\Pictures\hills.jpg")),
            background_dim: 0.35,
            ..Default::default()
        };
        let identity = MonitorIdentity {
            manufacturer_id: "GBT".into(),
            product_code: 9218,
            serial: "22350B003157".into(),
            is_fallback: false,
        };
        settings.set_monitor_state(
            &identity,
            MonitorState {
                brightness_value: 42,
                normalized_value: 0.42,
                last_set_by_app: true,
                write_confirmed: true,
                timestamp: 1,
                last_seen_device_path: r"DISPLAY\GBT2402\5&145fde2&5&UID24837".into(),
            },
        );

        settings.save_to(&path).unwrap();
        assert!(!path.with_extension("json.tmp").exists());
        let (loaded, warning) = Settings::load_from(&path);
        assert!(warning.is_none());
        assert_eq!(loaded, settings);
    }

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
        let settings = Settings {
            theme: ThemeMode::Dark,
            ..Default::default()
        };
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
        assert_eq!(settings.background_image, None);
        assert_eq!(settings.background_dim, default_background_dim());
    }

    #[test]
    fn test_theme_auto_follows_system() {
        assert!(!ThemeMode::Light.is_dark(true));
        assert!(ThemeMode::Dark.is_dark(false));
        assert!(ThemeMode::Auto.is_dark(true));
        assert!(!ThemeMode::Auto.is_dark(false));
        // Glass is a bright theme whatever Windows is set to.
        assert!(!ThemeMode::Glass.is_dark(true));
        assert!(!ThemeMode::Glass.is_dark(false));
    }

    #[test]
    fn test_glass_theme_round_trips_through_json() {
        let json = serde_json::to_string(&ThemeMode::Glass).unwrap();
        assert_eq!(json, "\"Glass\"");
        assert_eq!(serde_json::from_str::<ThemeMode>(&json).unwrap(), ThemeMode::Glass);
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