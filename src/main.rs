#![windows_subsystem = "windows"]

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
mod actions;
mod hotkeys;
mod worker;
mod log;
mod instance;
mod background;

use std::cell::{RefCell, RefMut};

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
    Win32::System::SystemServices::MK_SHIFT,
};

use crate::settings::{Settings, ThemeMode};
use crate::actions::Action;
use crate::layout::Hit;

/// Everything the window works with. `main` owns it; the window keeps a pointer to it
/// in `GWLP_USERDATA` from `WM_NCCREATE` to `WM_NCDESTROY`. Worker threads never touch
/// it, they only post messages.
struct App {
    state: AppState,
    renderer: render::Renderer,
    hotkeys: hotkeys::HotkeyManager,
    /// Bumped by every `load_background`; only the newest decode result is used.
    background_generation: usize,
}

/// Posted once the window is up: find the monitors and start their workers.
const WM_APP_INIT: u32 = WM_APP;

fn main() -> Result<()> {
    // Held until main returns; a second launch only brings this window forward.
    let Some(_instance) = instance::acquire() else {
        instance::activate_existing();
        return Ok(());
    };
    let started = std::time::Instant::now();
    log::reset();
    unsafe {
        win::init_com()?;
        win::init_dpi()?;

        // Monitors are found after the window is up, so it appears without waiting on them.
        let (settings, config_warning) = Settings::load();
        let system_dark = Settings::is_system_dark_mode();
        let dark = settings.theme.is_dark(system_dark);
        let app = RefCell::new(App {
            state: AppState {
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
            },
            renderer: render::Renderer::new()?,
            hotkeys: hotkeys::HotkeyManager::new(),
            background_generation: 0,
        });

        let instance = GetModuleHandleW(None)?.into();
        // Tint the frame before the window is shown, so it never flashes the system colors.
        let frame = render::frame_colors(&app.borrow().state);
        // `app` outlives the window: the loop below only ends once the window is destroyed.
        let app_ptr = &app as *const RefCell<App> as *const std::ffi::c_void;
        // The window is created without a menu bar; its controls are drawn in the client area.
        let hwnd = window::create(instance, "Brightness Control", Some(wnd_proc), None, dark, frame, Some(app_ptr))?;
        // Paint now; otherwise the first WM_PAINT would wait behind WM_APP_INIT.
        let _ = UpdateWindow(hwnd);
        debug_log!("window shown after {:?}", started.elapsed());
        let _ = PostMessageW(Some(hwnd), WM_APP_INIT, WPARAM(0), LPARAM(0));

        {
            let mut guard = app.borrow_mut();
            let App { state, hotkeys, .. } = &mut *guard;
            hotkeys.register_all(hwnd, &state.settings.hotkeys);
            hotkeys.sync_status(state);

            // A config error is more important; failed hotkeys also show as HOTKEY ERROR.
            let failed_count = state.hotkey_status.values().filter(|&&v| !v).count();
            if failed_count > 0 && state.status_message == "Ready" {
                state.status_message = format!("Warning: {} hotkeys failed to register", failed_count);
            }
        }

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    Ok(())
}

/// The window's `App`. `None` before `WM_NCCREATE`, after `WM_NCDESTROY`, and while a
/// message further up the stack has it borrowed. Don't hold the borrow across calls that
/// send messages back to the window (`SetWindowPos`, `DefWindowProcW`, `MessageBoxW`,
/// capture and frame changes), or those messages are skipped.
unsafe fn borrow_app<'a>(window: HWND) -> Option<RefMut<'a, App>> {
    let cell = (GetWindowLongPtrW(window, GWLP_USERDATA) as *const RefCell<App>).as_ref()?;
    let app = cell.try_borrow_mut();
    if app.is_err() {
        debug_log!("skipped a re-entrant message: the App is already borrowed");
    }
    app.ok()
}

const WM_MOUSELEAVE: u32 = 0x02A3;

/// One-shot timer that saves settings once writes have settled.
const SAVE_TIMER_ID: usize = 1;
const SAVE_DELAY_MS: u32 = 1000;

/// One-shot timer that refreshes the monitors once a burst of display changes settles.
const REFRESH_TIMER_ID: usize = 2;
const REFRESH_DELAY_MS: u32 = 500;

/// One-shot timer that decodes the background again once the window stops growing.
const BACKGROUND_TIMER_ID: usize = 3;
const BACKGROUND_DELAY_MS: u32 = 300;

/// Saves monitor states if a write changed them since the last save.
fn save_if_dirty(state: &mut AppState) {
    if state.settings_dirty {
        state.persist_monitor_states();
        let _ = state.settings.save();
    }
}

/// Finds the monitors and starts their workers. Only the registry and SetupDi are
/// read here; each worker talks to its monitor on its own thread.
unsafe fn start_monitors(window: HWND, state: &mut AppState) {
    // Smart Restore reads the saved states, so save what the old monitors wrote first.
    save_if_dirty(state);
    let _ = KillTimer(Some(window), REFRESH_TIMER_ID);
    let started = std::time::Instant::now();
    state.replace_monitors(monitors::enumerate_monitors());
    state.start_workers(window);
    debug_log!("found {} monitors in {:?}", state.monitors.len(), started.elapsed());
    let _ = InvalidateRect(Some(window), None, false);
}

/// Finds the monitors again (after a display change) and reports the count.
unsafe fn refresh_monitors(window: HWND, reason: &str) {
    if let Some(mut app) = borrow_app(window) {
        let state = &mut app.state;
        start_monitors(window, state);
        state.status_message = format!("{reason} (found {})", state.monitors.len());
    }
}

unsafe fn set_theme(window: HWND, theme: ThemeMode) {
    let Some(mut app) = borrow_app(window) else {
        return;
    };
    app.state.settings.theme = theme;
    let _ = app.state.settings.save();
    app.state.status_message = format!("Theme set to {:?}", theme);
    let dark = app.state.is_dark();
    let frame = render::frame_colors(&app.state);
    drop(app);
    // Redrawing the frame can send messages back to the window, so after the borrow.
    window::set_dark_title_bar(window, dark);
    window::set_frame_colors(window, frame);
    let _ = InvalidateRect(Some(window), None, false);
}

/// `lparam` of `WM_SETTINGCHANGE` names what changed; "ImmersiveColorSet" covers the
/// light/dark app mode.
unsafe fn is_color_set_change(lparam: LPARAM) -> bool {
    let name = PCWSTR(lparam.0 as *const u16);
    !name.is_null() && name.as_wide() == w!("ImmersiveColorSet").as_wide()
}

unsafe fn client_size(window: HWND) -> (u32, u32) {
    let mut rect = RECT::default();
    let _ = GetClientRect(window, &mut rect);
    ((rect.right - rect.left).max(1) as u32, (rect.bottom - rect.top).max(1) as u32)
}

/// Starts decoding the configured background at the current client size, or clears it.
/// The picture shows up with `WM_APP_BACKGROUND_READY`; until then the old one stays.
unsafe fn load_background(window: HWND, app: &mut App) {
    app.background_generation = app.background_generation.wrapping_add(1);
    let _ = KillTimer(Some(window), BACKGROUND_TIMER_ID);
    match app.state.settings.background_image.clone() {
        Some(path) => {
            let (width, height) = client_size(window);
            background::load(window, app.background_generation, path, width, height);
        }
        None => app.renderer.set_background(None),
    }
}

/// Asks for a picture to show behind the sliders, then stores and loads it.
unsafe fn pick_background(window: HWND) {
    let current =
        borrow_app(window).and_then(|app| app.state.settings.background_image.clone());
    // The dialog runs its own message loop, so the App must not be borrowed here.
    if let Some(path) = background::pick_file(window, current.as_deref()) {
        set_background(window, Some(path));
    }
}

/// Stores the new background path, saves it and starts loading the picture.
unsafe fn set_background(window: HWND, path: Option<std::path::PathBuf>) {
    let Some(mut app) = borrow_app(window) else {
        return;
    };
    app.state.status_message = match &path {
        Some(path) => format!("Background: {}", path.file_name().unwrap_or_default().to_string_lossy()),
        None => "Background removed".to_string(),
    };
    app.state.settings.background_image = path;
    let _ = app.state.settings.save();
    load_background(window, &mut app);
    let _ = InvalidateRect(Some(window), None, false);
}

extern "system" fn wnd_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match message {
            WM_NCCREATE => {
                // The first message that carries `CreateWindowExW`'s lpParam: the `App`.
                let create = &*(lparam.0 as *const CREATESTRUCTW);
                SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
                DefWindowProcW(window, message, wparam, lparam)
            }
            WM_NCDESTROY => {
                SetWindowLongPtrW(window, GWLP_USERDATA, 0);
                DefWindowProcW(window, message, wparam, lparam)
            }
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
                if let Some(mut app) = borrow_app(window)
                    && app.state.commit_edit()
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
            // Direct2D paints the whole client area, so skip the erase that would flash first.
            WM_ERASEBKGND => LRESULT(1),
            WM_SETTINGCHANGE => {
                if is_color_set_change(lparam)
                    && let Some(mut app) = borrow_app(window)
                {
                    let was_dark = app.state.is_dark();
                    app.state.system_dark = Settings::is_system_dark_mode();
                    let dark = app.state.is_dark();
                    drop(app);
                    if dark != was_dark {
                        window::set_dark_title_bar(window, dark);
                        let _ = InvalidateRect(Some(window), None, false);
                    }
                }
                DefWindowProcW(window, message, wparam, lparam)
            }
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                BeginPaint(window, &mut ps);
                if let Some(mut app) = borrow_app(window) {
                    let App { state, renderer, .. } = &mut *app;
                    let _ = renderer.render(window, state);
                }
                let _ = EndPaint(window, &ps);
                LRESULT(0)
            }
            WM_GETMINMAXINFO => {
                // Keep the window from being dragged smaller than the layout can hold.
                let info = &mut *(lparam.0 as *mut MINMAXINFO);
                let (width, height) = window::min_window_size(GetDpiForWindow(window));
                info.ptMinTrackSize = POINT { x: width, y: height };
                LRESULT(0)
            }
            WM_SIZE => {
                // lParam holds the new client width and height in pixels.
                if let Some(mut app) = borrow_app(window) {
                    let width = (lparam.0 & 0xFFFF) as u32;
                    let height = ((lparam.0 >> 16) & 0xFFFF) as u32;
                    app.renderer.resize(width, height);
                    // A bigger window needs a bigger copy of the picture; wait for the drag to end.
                    if !app.renderer.background_covers(width, height) {
                        SetTimer(Some(window), BACKGROUND_TIMER_ID, BACKGROUND_DELAY_MS, None);
                    }
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
                // Captured so a drag that leaves the window still reaches the slider.
                SetCapture(window);
                handle_input(window, x, y, true);
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
                if let Some(mut app) = borrow_app(window) {
                    app.state.hover_monitor_idx = None;
                    let _ = InvalidateRect(Some(window), None, false);
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                if let Some(mut app) = borrow_app(window) {
                    app.state.active_monitor_idx = None;
                    let _ = InvalidateRect(Some(window), None, false);
                }
                let _ = ReleaseCapture();
                LRESULT(0)
            }
            worker::WM_APP_WRITE_DONE => {
                if let Some(mut app) = borrow_app(window) {
                    let state = &mut app.state;
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
                if let Some(mut app) = borrow_app(window) {
                    save_if_dirty(&mut app.state);
                }
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
            WM_TIMER if wparam.0 == BACKGROUND_TIMER_ID => {
                if let Some(mut app) = borrow_app(window) {
                    load_background(window, &mut app);
                }
                LRESULT(0)
            }
            WM_APP_INIT => {
                if let Some(mut app) = borrow_app(window) {
                    start_monitors(window, &mut app.state);
                    load_background(window, &mut app);
                }
                LRESULT(0)
            }
            background::WM_APP_BACKGROUND_READY => {
                // Always take the box, even for a stale generation, so it's freed.
                let loaded = background::take_loaded(lparam);
                if let Some(mut app) = borrow_app(window)
                    && wparam.0 == app.background_generation
                {
                    match *loaded {
                        Ok(picture) => app.renderer.set_background(Some(picture)),
                        Err(err) => {
                            app.renderer.set_background(None);
                            app.state.status_message = format!("Background image not loaded: {}", err.trim_end());
                        }
                    }
                    // The window may have grown while the picture was decoding.
                    let (width, height) = client_size(window);
                    if !app.renderer.background_covers(width, height) {
                        SetTimer(Some(window), BACKGROUND_TIMER_ID, BACKGROUND_DELAY_MS, None);
                    }
                    let _ = InvalidateRect(Some(window), None, false);
                }
                LRESULT(0)
            }
            worker::WM_APP_MONITOR_READY => {
                // Always take the box, even for a stale generation, so it's freed.
                let ready = worker::take_ready(lparam);
                if let Some(mut app) = borrow_app(window) {
                    let state = &mut app.state;
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
                if let Some(mut app) = borrow_app(window) {
                    save_if_dirty(&mut app.state);
                }
                // Destroys the window, which sends WM_DESTROY.
                DefWindowProcW(window, message, wparam, lparam)
            }
            WM_ENDSESSION => {
                if wparam.0 != 0
                    && let Some(mut app) = borrow_app(window)
                {
                    save_if_dirty(&mut app.state);
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                if let Some(mut app) = borrow_app(window) {
                    app.hotkeys.unregister_all(window);
                }
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(window, message, wparam, lparam),
        }
    }
}

unsafe fn handle_keydown(window: HWND, vk: VIRTUAL_KEY) {
    if let Some(mut app) = borrow_app(window) {
        let state = &mut app.state;
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
    if let Some(mut app) = borrow_app(window)
        && let Some(ch) = char::from_u32(unit)
        && app.state.edit_input(ch)
    {
        let _ = InvalidateRect(Some(window), None, false);
    }
}

unsafe fn handle_wheel(window: HWND, delta: i16, keystate: u16) {
    if let Some(mut app) = borrow_app(window) {
        let state = &mut app.state;
        // Only Shift is read, so wheel use never clashes with hotkey modifiers.
        let shift = u32::from(keystate) & MK_SHIFT.0 != 0;
        if let Some(targets) = state.apply_wheel(delta, shift, std::time::Instant::now()) {
            state.request_brightness(&targets);
            let _ = InvalidateRect(Some(window), None, false);
        }
    }
}

unsafe fn handle_hotkey(window: HWND, action_id: i32) {
    if let Some(mut app) = borrow_app(window) {
        let state = &mut app.state;
        if let Some(action) = Action::from_i32(action_id) {
            let mut targets = state.get_target_indices();
            
            // Refine targeting: If default fallback was chosen (no UI hover/focus), check system cursor
            if !state.lock_mode && state.hover_monitor_idx.is_none() && state.active_monitor_idx.is_none() {
                let hmonitor = monitors::get_monitor_handle_at_cursor();
                if hmonitor != 0
                    && let Some(idx) = state.monitors.iter().position(|m| m.hmonitor == hmonitor)
                {
                    targets = vec![idx];
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

/// A control picked in the client area, run once the `App` borrow has ended: setting the
/// theme and opening the picture dialog both send messages back to the window.
#[derive(Clone, Copy)]
enum Command {
    SetTheme(ThemeMode),
    PickPicture,
    RemovePicture,
}

unsafe fn handle_input(window: HWND, x: i32, y: i32, is_down: bool) {
    // Set by a click on one of the option buttons; run once the borrow below has ended.
    let mut command = None;
    if let Some(mut app) = borrow_app(window) {
        let App { state, .. } = &mut *app;
        // Mouse coordinates are physical pixels; the layout is in DIPs.
        let dpi = GetDpiForWindow(window);
        let mut rect = RECT::default();
        let _ = GetClientRect(window, &mut rect);
        let width = layout::px_to_dip(rect.right - rect.left, dpi);
        let height = layout::px_to_dip(rect.bottom - rect.top, dpi);
        let layout = layout::compute(
            width,
            height,
            state.monitors.len(),
            state.settings.background_image.is_some(),
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

        // If clicking down, find which control was hit. Monitors still being probed (or that
        // can't be controlled) ignore clicks. Clicking anywhere but the label being edited
        // commits the typed value. Pressing on a track starts a drag.
        if is_down {
            let hit = hit.filter(|h| match h.index() {
                Some(idx) => state.monitors.get(idx).is_some_and(|m| m.accepts_input()),
                // The window's own controls are always clickable.
                None => true,
            });
            match hit {
                Some(Hit::Theme(index)) => {
                    command = ThemeMode::ALL.get(index).copied().map(Command::SetTheme);
                }
                Some(Hit::Sync) => {
                    state.lock_mode = !state.lock_mode;
                    state.status_message = if state.lock_mode {
                        "Sync Mode Active".to_string()
                    } else {
                        "Sync Mode Disabled".to_string()
                    };
                }
                Some(Hit::Picture) => command = Some(Command::PickPicture),
                Some(Hit::RemovePicture) => command = Some(Command::RemovePicture),
                Some(Hit::Value(idx)) => state.begin_edit(idx),
                Some(Hit::Track(idx)) => {
                    state.commit_edit();
                    state.active_monitor_idx = Some(idx);
                }
                None => {
                    state.commit_edit();
                }
            }
            let _ = InvalidateRect(Some(window), None, false);
        }

        // Processing movement/drag
        if let Some(idx) = state.active_monitor_idx {
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

    // The theme change and the picture dialog both send messages back to the window, so they
    // run after the borrow above has ended.
    match command {
        Some(Command::SetTheme(theme)) => set_theme(window, theme),
        Some(Command::PickPicture) => pick_background(window),
        Some(Command::RemovePicture) => set_background(window, None),
        None => {}
    }
}
