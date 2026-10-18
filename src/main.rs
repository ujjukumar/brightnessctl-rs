#![windows_subsystem = "windows"]

#![allow(static_mut_refs)]
#![allow(unsafe_op_in_unsafe_fn)]

mod win;
mod window;
mod monitors;
mod brightness;
mod edid;
mod wmi;
mod state;
mod render;
mod layout;
mod settings;
mod menu;
mod actions;
mod hotkeys;
mod worker;
mod log;

use crate::log::debug_log;
use crate::state::AppState;

use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::UI::WindowsAndMessaging::*,
    Win32::Graphics::Gdi::{PAINTSTRUCT, BeginPaint, EndPaint, InvalidateRect, UpdateWindow},
    Win32::UI::Input::KeyboardAndMouse::{SetCapture, ReleaseCapture, TrackMouseEvent, TRACKMOUSEEVENT, TME_LEAVE, VIRTUAL_KEY, VK_ESCAPE, VK_RETURN},
    Win32::UI::HiDpi::GetDpiForWindow,
    Win32::System::LibraryLoader::GetModuleHandleW,
};

use crate::settings::{Settings, ThemeMode};
use crate::actions::Action;
use crate::layout::Hit;

static mut APP_STATE: Option<AppState> = None;
static mut RENDERER: Option<render::Renderer> = None;
static mut HOTKEY_MANAGER: Option<hotkeys::HotkeyManager> = None;

// Input state
static mut DRAGGING_MONITOR_IDX: Option<usize> = None;

/// Posted once the window is up: find the monitors and start their workers.
const WM_APP_INIT: u32 = WM_APP;

fn main() -> Result<()> {
    let started = std::time::Instant::now();
    log::reset();
    unsafe {
        win::init_com()?;
        win::init_dpi()?;

        // Monitors are found after the window is up, so it appears without waiting on them.
        let (settings, config_warning) = Settings::load();
        let system_dark = Settings::is_system_dark_mode();
        let dark = settings.theme.is_dark(system_dark);
        APP_STATE = Some(AppState {
            monitors: Vec::new(),
            settings,
            status_message: config_warning.unwrap_or_else(|| "Ready".to_string()),
            hover_monitor_idx: None,
            active_monitor_idx: None,
            lock_mode: false,
            hotkey_status: std::collections::HashMap::new(),
            last_wheel_time: None,
            editing_monitor: None,
            edit_buffer: String::new(),
            worker_generation: 0,
            settings_dirty: false,
            system_dark,
        });

        RENDERER = Some(render::Renderer::new()?);

        let instance = GetModuleHandleW(None)?.into();
        let hmenu = menu::create_menu_bar()?;
        let hwnd = window::create(instance, "Brightness Control", Some(wnd_proc), Some(hmenu), dark)?;
        // Paint now; otherwise the first WM_PAINT would wait behind WM_APP_INIT.
        let _ = UpdateWindow(hwnd);
        debug_log!("window shown after {:?}", started.elapsed());
        let _ = PostMessageW(Some(hwnd), WM_APP_INIT, WPARAM(0), LPARAM(0));

        // Initialize hotkeys
        let mut hkm = hotkeys::HotkeyManager::new();
        if let Some(state) = APP_STATE.as_mut() {
            hkm.register_all(hwnd, &state.settings.hotkeys);
            hkm.sync_status(state);
            
            // A config error is more important; failed hotkeys also show as HOTKEY ERROR.
            let failed_count = state.hotkey_status.values().filter(|&&v| !v).count();
            if failed_count > 0 && state.status_message == "Ready" {
                state.status_message = format!("Warning: {} hotkeys failed to register", failed_count);
            }
        }
        HOTKEY_MANAGER = Some(hkm);

        if let Some(state) = APP_STATE.as_ref() {
            update_theme_menu(hwnd, state);
        }

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    Ok(())
}

const WM_MOUSELEAVE: u32 = 0x02A3;

/// One-shot timer that saves settings once writes have settled.
const SAVE_TIMER_ID: usize = 1;
const SAVE_DELAY_MS: u32 = 1000;

/// One-shot timer that refreshes the monitors once a burst of display changes settles.
const REFRESH_TIMER_ID: usize = 2;
const REFRESH_DELAY_MS: u32 = 500;

/// Saves monitor states if a write changed them since the last save.
unsafe fn save_if_dirty() {
    if let Some(state) = APP_STATE.as_mut()
        && state.settings_dirty
    {
        state.persist_monitor_states();
        let _ = state.settings.save();
    }
}

/// Finds the monitors and starts their workers. Only the registry and SetupDi are
/// read here; each worker talks to its monitor on its own thread.
unsafe fn start_monitors(window: HWND) {
    // Smart Restore reads the saved states, so save what the old monitors wrote first.
    save_if_dirty();
    let _ = KillTimer(Some(window), REFRESH_TIMER_ID);
    DRAGGING_MONITOR_IDX = None;
    if let Some(state) = APP_STATE.as_mut() {
        let started = std::time::Instant::now();
        state.replace_monitors(monitors::enumerate_monitors());
        state.start_workers(window);
        debug_log!("found {} monitors in {:?}", state.monitors.len(), started.elapsed());
        let _ = InvalidateRect(Some(window), None, false);
    }
}

/// Finds the monitors again (menu or display change) and reports the count.
unsafe fn refresh_monitors(window: HWND, reason: &str) {
    start_monitors(window);
    if let Some(state) = APP_STATE.as_mut() {
        state.status_message = format!("{reason} (found {})", state.monitors.len());
    }
}

unsafe fn update_theme_menu(window: HWND, state: &AppState) {
    let hmenu = GetMenu(window);
    let theme = state.settings.theme;
    
    let _ = CheckMenuItem(hmenu, menu::IDM_THEME_AUTO as u32, if theme == ThemeMode::Auto { MF_CHECKED } else { MF_UNCHECKED }.0);
    let _ = CheckMenuItem(hmenu, menu::IDM_THEME_LIGHT as u32, if theme == ThemeMode::Light { MF_CHECKED } else { MF_UNCHECKED }.0);
    let _ = CheckMenuItem(hmenu, menu::IDM_THEME_DARK as u32, if theme == ThemeMode::Dark { MF_CHECKED } else { MF_UNCHECKED }.0);
}

unsafe fn set_theme(window: HWND, theme: ThemeMode) {
    if let Some(state) = APP_STATE.as_mut() {
        state.settings.theme = theme;
        let _ = state.settings.save();
        update_theme_menu(window, state);
        window::set_dark_title_bar(window, state.is_dark());
        state.status_message = format!("Theme set to {:?}", theme);
        let _ = InvalidateRect(Some(window), None, false);
    }
}

/// `lparam` of `WM_SETTINGCHANGE` names what changed; "ImmersiveColorSet" covers the
/// light/dark app mode.
unsafe fn is_color_set_change(lparam: LPARAM) -> bool {
    let name = PCWSTR(lparam.0 as *const u16);
    !name.is_null() && name.as_wide() == w!("ImmersiveColorSet").as_wide()
}

extern "system" fn wnd_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match message {
            WM_KEYDOWN => {
                handle_keydown(window, VIRTUAL_KEY(wparam.0 as u16));
                LRESULT(0)
            }
            WM_CHAR => {
                handle_char(window, wparam.0 as u32);
                LRESULT(0)
            }
            WM_KILLFOCUS => {
                // Leaving the window commits a typed value, like clicking elsewhere.
                if let Some(state) = APP_STATE.as_mut()
                    && state.commit_edit()
                {
                    let _ = InvalidateRect(Some(window), None, false);
                }
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                let delta = (wparam.0 >> 16) as i16;
                let keystate = (wparam.0 & 0xffff) as u16;
                handle_wheel(window, delta, keystate);
                LRESULT(0)
            }
            WM_HOTKEY => {
                let action_id = wparam.0 as i32;
                handle_hotkey(window, action_id);
                LRESULT(0)
            }
            WM_COMMAND => {
                let id = (wparam.0 & 0xffff) as u16;
                match id {
                    menu::IDM_EXIT => {
                        let _ = PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0));
                    }
                    menu::IDM_REFRESH => refresh_monitors(window, "Monitors refreshed"),
                    menu::IDM_LOCK_MODE => {
                        if let Some(state) = APP_STATE.as_mut() {
                            state.lock_mode = !state.lock_mode;
                            
                            let hmenu = GetMenu(window);
                            let check = if state.lock_mode { MF_CHECKED } else { MF_UNCHECKED };
                            let _ = CheckMenuItem(hmenu, menu::IDM_LOCK_MODE as u32, check.0);

                            state.status_message = if state.lock_mode {
                                "Sync Mode Active".to_string()
                            } else {
                                "Sync Mode Disabled".to_string()
                            };
                            let _ = InvalidateRect(Some(window), None, false);
                        }
                    }
                    menu::IDM_THEME_AUTO => set_theme(window, ThemeMode::Auto),
                    menu::IDM_THEME_LIGHT => set_theme(window, ThemeMode::Light),
                    menu::IDM_THEME_DARK => set_theme(window, ThemeMode::Dark),
                    menu::IDM_ABOUT => {
                        let _ = MessageBoxW(
                            Some(window),
                            w!("Brightness Control v0.1.0\nA native Windows 11 utility."),
                            w!("About"),
                            MB_OK | MB_ICONINFORMATION,
                        );
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            // Direct2D paints the whole client area, so skip the erase that would flash first.
            WM_ERASEBKGND => LRESULT(1),
            WM_SETTINGCHANGE => {
                if is_color_set_change(lparam)
                    && let Some(state) = APP_STATE.as_mut()
                {
                    let was_dark = state.is_dark();
                    state.system_dark = Settings::is_system_dark_mode();
                    if state.is_dark() != was_dark {
                        window::set_dark_title_bar(window, state.is_dark());
                        let _ = InvalidateRect(Some(window), None, false);
                    }
                }
                DefWindowProcW(window, message, wparam, lparam)
            }
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                BeginPaint(window, &mut ps);
                if let Some(renderer) = RENDERER.as_mut() {
                    if let Some(state) = APP_STATE.as_ref() {
                        let _ = renderer.render(window, state);
                    }
                }
                let _ = EndPaint(window, &ps);
                LRESULT(0)
            }
            WM_SIZE => {
                // lParam holds the new client width and height in pixels.
                if let Some(renderer) = RENDERER.as_mut() {
                    let width = (lparam.0 & 0xFFFF) as u32;
                    let height = ((lparam.0 >> 16) & 0xFFFF) as u32;
                    renderer.resize(width, height);
                }
                let _ = InvalidateRect(Some(window), None, false);
                LRESULT(0)
            }
            WM_DPICHANGED => {
                // lParam points to the size and position Windows suggests for the new DPI.
                let suggested = &*(lparam.0 as *const RECT);
                let _ = SetWindowPos(
                    window,
                    None,
                    suggested.left,
                    suggested.top,
                    suggested.right - suggested.left,
                    suggested.bottom - suggested.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                let _ = InvalidateRect(Some(window), None, false);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let x = (lparam.0 as i16) as i32;
                let y = ((lparam.0 >> 16) as i16) as i32;
                handle_input(window, x, y, true);
                SetCapture(window);
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: window,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut tme);

                let x = (lparam.0 as i16) as i32;
                let y = ((lparam.0 >> 16) as i16) as i32;
                handle_input(window, x, y, false);
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                if let Some(state) = APP_STATE.as_mut() {
                    state.hover_monitor_idx = None;
                    let _ = InvalidateRect(Some(window), None, false);
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                DRAGGING_MONITOR_IDX = None;

                if let Some(state) = APP_STATE.as_mut() {
                    state.active_monitor_idx = None;
                    let _ = InvalidateRect(Some(window), None, false);
                }
                let _ = ReleaseCapture();
                LRESULT(0)
            }
            worker::WM_APP_WRITE_DONE => {
                if let Some(state) = APP_STATE.as_mut() {
                    let (generation, idx) = worker::unpack_target(wparam.0);
                    let result = worker::WriteResult::from_lparam(lparam.0);
                    if state.handle_write_result(generation, idx, result) {
                        let _ = InvalidateRect(Some(window), None, false);
                    }
                    if state.settings_dirty {
                        // Re-arming replaces the pending timer, so a burst of writes saves once.
                        SetTimer(Some(window), SAVE_TIMER_ID, SAVE_DELAY_MS, None);
                    }
                }
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == SAVE_TIMER_ID => {
                let _ = KillTimer(Some(window), SAVE_TIMER_ID);
                save_if_dirty();
                LRESULT(0)
            }
            WM_DISPLAYCHANGE => {
                // Monitors were added, removed or rearranged. Re-arming the timer means a
                // burst of changes refreshes once, after Windows has settled.
                SetTimer(Some(window), REFRESH_TIMER_ID, REFRESH_DELAY_MS, None);
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == REFRESH_TIMER_ID => {
                refresh_monitors(window, "Displays changed");
                LRESULT(0)
            }
            WM_APP_INIT => {
                start_monitors(window);
                LRESULT(0)
            }
            worker::WM_APP_MONITOR_READY => {
                // Always take the box, even for a stale generation, so it's freed.
                let ready = worker::take_ready(lparam);
                if let Some(state) = APP_STATE.as_mut() {
                    let (generation, idx) = worker::unpack_target(wparam.0);
                    if state.handle_monitor_ready(generation, idx, *ready) {
                        let _ = InvalidateRect(Some(window), None, false);
                    }
                    if state.settings_dirty {
                        SetTimer(Some(window), SAVE_TIMER_ID, SAVE_DELAY_MS, None);
                    }
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                save_if_dirty();
                DefWindowProcW(window, message, wparam, lparam)
            }
            WM_ENDSESSION => {
                if wparam.0 != 0 {
                    save_if_dirty();
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                if let Some(hkm) = HOTKEY_MANAGER.as_mut() {
                    hkm.unregister_all(window);
                }
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(window, message, wparam, lparam),
        }
    }
}

unsafe fn handle_keydown(window: HWND, vk: VIRTUAL_KEY) {
    if let Some(state) = APP_STATE.as_mut() {
        let changed = match vk {
            VK_RETURN => state.commit_edit(),
            VK_ESCAPE => state.cancel_edit(),
            _ => false,
        };
        if changed {
            let _ = InvalidateRect(Some(window), None, false);
        }
    }
}

/// `unit` is a UTF-16 code unit from `WM_CHAR`; surrogates never map to a digit.
unsafe fn handle_char(window: HWND, unit: u32) {
    if let Some(state) = APP_STATE.as_mut()
        && let Some(ch) = char::from_u32(unit)
        && state.edit_input(ch)
    {
        let _ = InvalidateRect(Some(window), None, false);
    }
}

unsafe fn handle_wheel(window: HWND, delta: i16, keystate: u16) {
    if let Some(state) = APP_STATE.as_mut() {
        if let Some(hover_idx) = state.hover_monitor_idx {
            // Rate limiting
            let now = std::time::Instant::now();
            if let Some(last) = state.last_wheel_time {
                if now.duration_since(last) < std::time::Duration::from_millis(50) {
                    return; // Too fast
                }
            }
            state.last_wheel_time = Some(now);

            // Shift for fine (0.01), else coarse (0.05)
            let step = if (keystate & 0x0004) != 0 { 0.01 } else { 0.05 };
            let direction = if delta > 0 { 1.0 } else { -1.0 };
            let change = direction * step;

            let targets = if state.lock_mode {
                (0..state.monitors.len()).collect()
            } else {
                vec![hover_idx]
            };

            state.apply_step(&targets, change);
            state.request_brightness(&targets);

            let _ = InvalidateRect(Some(window), None, false);
        }
    }
}

unsafe fn handle_hotkey(window: HWND, action_id: i32) {
    if let Some(state) = APP_STATE.as_mut() {
        if let Some(action) = Action::from_i32(action_id) {
            let mut targets = state.get_target_indices();
            
            // Refine targeting: If default fallback was chosen (no UI hover/focus), check system cursor
            if !state.lock_mode && state.hover_monitor_idx.is_none() && state.active_monitor_idx.is_none() {
                let hmonitor = monitors::get_monitor_handle_at_cursor();
                if hmonitor != 0 {
                    if let Some(idx) = state.monitors.iter().position(|m| m.hmonitor == hmonitor) {
                        targets = vec![idx];
                    }
                }
            }

            if targets.is_empty() { return; }

            match action {
                Action::StepUp => {
                    let step = state.settings.step_size;
                    state.apply_step(&targets, step);
                }
                Action::StepDown => {
                    let step = -state.settings.step_size;
                    state.apply_step(&targets, step);
                }
                Action::CyclePresets => {
                    state.cycle_presets(&targets);
                }
            }
            state.request_brightness(&targets);

            state.status_message = format!("Hotkey: {:?}", action);
            let _ = InvalidateRect(Some(window), None, false);
        }
    }
}

unsafe fn handle_input(window: HWND, x: i32, y: i32, is_down: bool) {
    if let Some(state) = APP_STATE.as_mut() {
        // Mouse coordinates are physical pixels; the layout is in DIPs.
        let dpi = GetDpiForWindow(window);
        let mut rect = RECT::default();
        let _ = GetClientRect(window, &mut rect);
        let layout = layout::compute(
            layout::px_to_dip(rect.right - rect.left, dpi),
            layout::px_to_dip(rect.bottom - rect.top, dpi),
            state.monitors.len(),
        );
        let x = layout::px_to_dip(x, dpi);
        let y = layout::px_to_dip(y, dpi);

        let hit = layout.hit_test(x, y);
        let hover_idx = match hit {
            Some(Hit::Track(idx)) => Some(idx),
            _ => None,
        };

        if state.hover_monitor_idx != hover_idx {
            state.hover_monitor_idx = hover_idx;
            let _ = InvalidateRect(Some(window), None, false);
        }

        // If clicking down, find which slider or label. Monitors still being probed (or
        // that can't be controlled) ignore clicks. Clicking anywhere but the label being
        // edited commits the typed value.
        if is_down {
            let hit = hit.filter(|h| state.monitors.get(h.index()).is_some_and(|m| m.accepts_input()));
            match hit {
                Some(Hit::Value(idx)) => state.begin_edit(idx),
                Some(Hit::Track(idx)) => {
                    state.commit_edit();
                    DRAGGING_MONITOR_IDX = Some(idx);
                    state.active_monitor_idx = Some(idx);
                }
                None => {
                    state.commit_edit();
                }
            }
            let _ = InvalidateRect(Some(window), None, false);
        }

        // Processing movement/drag
        if let Some(idx) = DRAGGING_MONITOR_IDX {
            let (Some(row), Some(old_value)) = (layout.rows.get(idx), state.monitors.get(idx).map(|m| m.ui_value)) else {
                return;
            };
            let new_value = layout::value_at(&row.track, x);
            let delta = new_value - old_value;

            if delta.abs() > 0.001 {
                let targets: Vec<usize> = if state.lock_mode {
                    state.apply_sync_delta(idx, delta);
                    (0..state.monitors.len()).collect()
                } else {
                    state.set_absolute(&[idx], new_value);
                    vec![idx]
                };
                state.request_brightness(&targets);

                // Repaint
                let _ = InvalidateRect(Some(window), None, false);
            }
        }
    }
}