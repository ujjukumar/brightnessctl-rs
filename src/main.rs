#![allow(static_mut_refs)]
#![allow(unsafe_op_in_unsafe_fn)]

mod win;
mod window;
mod monitors;
mod brightness;
mod edid;
mod state;
mod render;
mod settings;
mod menu;
mod actions;
mod hotkeys;

use crate::state::AppState;

use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::UI::WindowsAndMessaging::*,
    Win32::Graphics::Gdi::{PAINTSTRUCT, BeginPaint, EndPaint, InvalidateRect},
    Win32::UI::Input::KeyboardAndMouse::{SetCapture, ReleaseCapture, TrackMouseEvent, TRACKMOUSEEVENT, TME_LEAVE},
    Win32::System::LibraryLoader::GetModuleHandleW,
};

use crate::settings::Settings;
use crate::actions::Action;

static mut APP_STATE: Option<AppState> = None;
static mut RENDERER: Option<render::Renderer> = None;
static mut HOTKEY_MANAGER: Option<hotkeys::HotkeyManager> = None;

// Input state
static mut DRAGGING_MONITOR_IDX: Option<usize> = None;

fn main() -> Result<()> {
    unsafe {
        win::init_com()?;
        win::init_dpi()?;

        // Initialize state
        let settings = Settings::load();
        let mut monitors = monitors::enumerate_monitors();
        let mut initial_brightness = Vec::new();

        for m in &mut monitors {
            let mut applied = false;
            
            // Smart Restore Logic
            if let Some(ident) = &m.identity {
                if let Some(saved) = settings.get_monitor_state(ident) {
                    if saved.last_set_by_app && saved.write_confirmed {
                        if brightness::set_brightness(m, saved.brightness_value) {
                            initial_brightness.push(saved.brightness_value as f32 / 100.0);
                            applied = true;
                        }
                    }
                }
            }

            if !applied {
                if let Some(b) = brightness::get_brightness(m) {
                    initial_brightness.push(b as f32 / 100.0);
                } else {
                    initial_brightness.push(0.5); // Default if read fails
                }
            }
        }

        APP_STATE = Some(AppState {
            monitors,
            brightness: initial_brightness,
            settings,
            status_message: "Ready".to_string(),
            hover_monitor_idx: None,
            active_monitor_idx: None,
            lock_mode: false,
            hotkey_status: std::collections::HashMap::new(),
        });

        RENDERER = Some(render::Renderer::new()?);

        let instance = GetModuleHandleW(None)?.into();
        let hmenu = menu::create_menu_bar()?;
        let hwnd = window::create(instance, "Brightness Control", Some(wnd_proc), Some(hmenu))?;

        // Initialize hotkeys
        let mut hkm = hotkeys::HotkeyManager::new();
        if let Some(state) = APP_STATE.as_mut() {
            hkm.register_all(hwnd, &state.settings.hotkeys);
            hkm.sync_status(state);
            
            let failed_count = state.hotkey_status.values().filter(|&&v| !v).count();
            if failed_count > 0 {
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

unsafe fn update_theme_menu(window: HWND, state: &AppState) {
    let hmenu = GetMenu(window);
    let theme = state.settings.theme;
    
    let _ = CheckMenuItem(hmenu, menu::IDM_THEME_AUTO as u32, if theme == crate::settings::ThemeMode::Auto { MF_CHECKED } else { MF_UNCHECKED }.0);
    let _ = CheckMenuItem(hmenu, menu::IDM_THEME_LIGHT as u32, if theme == crate::settings::ThemeMode::Light { MF_CHECKED } else { MF_UNCHECKED }.0);
    let _ = CheckMenuItem(hmenu, menu::IDM_THEME_DARK as u32, if theme == crate::settings::ThemeMode::Dark { MF_CHECKED } else { MF_UNCHECKED }.0);
}

extern "system" fn wnd_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match message {
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
                    menu::IDM_REFRESH => {
                        if let Some(state) = APP_STATE.as_mut() {
                            state.monitors = monitors::enumerate_monitors();
                            state.brightness.clear();
                            for m in &mut state.monitors {
                                if let Some(b) = brightness::get_brightness(m) {
                                    state.brightness.push(b as f32 / 100.0);
                                } else {
                                    state.brightness.push(0.5);
                                }
                            }
                            state.status_message = format!("Monitors refreshed (found {})", state.monitors.len());
                            let _ = InvalidateRect(Some(window), None, false);
                        }
                    }
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
                    menu::IDM_THEME_AUTO => {
                        if let Some(state) = APP_STATE.as_mut() {
                            state.settings.theme = crate::settings::ThemeMode::Auto;
                            let _ = state.settings.save();
                            update_theme_menu(window, state);
                            state.status_message = "Theme set to Auto".to_string();
                            let _ = InvalidateRect(Some(window), None, false);
                        }
                    }
                    menu::IDM_THEME_LIGHT => {
                        if let Some(state) = APP_STATE.as_mut() {
                            state.settings.theme = crate::settings::ThemeMode::Light;
                            let _ = state.settings.save();
                            update_theme_menu(window, state);
                            state.status_message = "Theme set to Light".to_string();
                            let _ = InvalidateRect(Some(window), None, false);
                        }
                    }
                    menu::IDM_THEME_DARK => {
                        if let Some(state) = APP_STATE.as_mut() {
                            state.settings.theme = crate::settings::ThemeMode::Dark;
                            let _ = state.settings.save();
                            update_theme_menu(window, state);
                            state.status_message = "Theme set to Dark".to_string();
                            let _ = InvalidateRect(Some(window), None, false);
                        }
                    }
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
                let dragged_idx = DRAGGING_MONITOR_IDX;
                DRAGGING_MONITOR_IDX = None;
                
                if let Some(state) = APP_STATE.as_mut() {
                    state.active_monitor_idx = None;
                    
                    if let Some(idx) = dragged_idx {
                        if state.lock_mode {
                             for (i, m) in state.monitors.iter_mut().enumerate() {
                                let val = state.brightness[i];
                                brightness::set_brightness_forced(m, (val * 100.0).round() as u32);
                            }
                        } else {
                             let val = state.brightness[idx];
                             let m = &mut state.monitors[idx];
                             brightness::set_brightness_forced(m, (val * 100.0).round() as u32);
                        }
                    }

                    // Persist monitor states
                    for (i, m) in state.monitors.iter().enumerate() {
                        if let Some(ident) = &m.identity {
                            let val = state.brightness[i];
                            let brightness_u32 = (val * 100.0).round() as u32;
                            state.settings.set_monitor_state(ident, m.to_state(brightness_u32));
                        }
                    }
                    let _ = state.settings.save();

                    let _ = InvalidateRect(Some(window), None, false);
                }
                let _ = ReleaseCapture();
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(window, message, wparam, lparam),
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
                    // Deferred to Phase 6
                    state.status_message = "Preset cycling not yet implemented".to_string();
                }
            }

            // Update hardware for all affected targets
            for &idx in &targets {
                if let Some(m) = state.monitors.get_mut(idx) {
                    let val = state.brightness[idx];
                    brightness::set_brightness(m, (val * 100.0).round() as u32);
                }
            }

            state.status_message = format!("Hotkey: {:?}", action);
            let _ = InvalidateRect(Some(window), None, false);
        }
    }
}

unsafe fn handle_input(window: HWND, x: i32, y: i32, is_down: bool) {
    if let Some(state) = APP_STATE.as_mut() {
        // Layout calculations MUST match render.rs
        let margin_x = 24.0;
        let mut cur_y = 24.0 + 48.0; // Header spacing
        let monitor_spacing = 48.0;
        
        // Get window width for layout
        let mut rect = windows::Win32::Foundation::RECT::default();
        let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(window, &mut rect);
        let width = (rect.right - rect.left) as f32;
        
        let slider_width = (width - 2.0 * margin_x).max(1.0);

        let mut hit_idx = None;
        for (i, _) in state.monitors.iter().enumerate() {
            let slider_top = cur_y + 24.0 + 8.0;
            let slider_bottom = slider_top + 4.0;
            
            let hit_top = slider_top - 12.0;
            let hit_bottom = slider_bottom + 12.0;
            
            if (y as f32) >= hit_top && (y as f32) <= hit_bottom {
                hit_idx = Some(i);
                break;
            }
            
            cur_y += 24.0 + monitor_spacing;
        }

        if state.hover_monitor_idx != hit_idx {
            state.hover_monitor_idx = hit_idx;
            let _ = InvalidateRect(Some(window), None, false);
        }

        // If clicking down, find which slider
        if is_down {
            if let Some(idx) = hit_idx {
                DRAGGING_MONITOR_IDX = Some(idx);
                state.active_monitor_idx = Some(idx);
                let _ = InvalidateRect(Some(window), None, false);
            }
        }

        // Processing movement/drag
        if let Some(idx) = DRAGGING_MONITOR_IDX {
            let new_pct = ((x as f32 - margin_x) / slider_width).clamp(0.0, 1.0);
            let old_pct = state.brightness[idx];
            let delta = new_pct - old_pct;

            if delta.abs() > 0.001 {
                if state.lock_mode {
                    state.apply_sync_delta(idx, delta);
                    // Hardware update for all
                    for (i, m) in state.monitors.iter_mut().enumerate() {
                        let val = state.brightness[i];
                        brightness::set_brightness(m, (val * 100.0).round() as u32);
                    }
                } else {
                    if (state.brightness[idx] - new_pct).abs() > 0.001 {
                        state.brightness[idx] = new_pct;
                        let m = &mut state.monitors[idx];
                        brightness::set_brightness(m, (new_pct * 100.0).round() as u32);
                    }
                }
                
                // Repaint
                let _ = InvalidateRect(Some(window), None, false);
            }
        }
    }
}