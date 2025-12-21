#![allow(static_mut_refs)]
#![allow(unsafe_op_in_unsafe_fn)]

mod win;
mod window;
mod monitors;
mod brightness;
mod state;
mod render;

use crate::state::AppState;

use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::UI::WindowsAndMessaging::*,
    Win32::Graphics::Gdi::{PAINTSTRUCT, BeginPaint, EndPaint, InvalidateRect},
    Win32::UI::Input::KeyboardAndMouse::{SetCapture, ReleaseCapture},
    Win32::System::SystemServices::MK_LBUTTON,
    Win32::System::LibraryLoader::GetModuleHandleW,
};

static mut APP_STATE: Option<AppState> = None;
static mut RENDERER: Option<render::Renderer> = None;

// Input state
static mut DRAGGING_MONITOR_IDX: Option<usize> = None;

fn main() -> Result<()> {
    unsafe {
        win::init_com()?;
        win::init_dpi()?;

        // Initialize state
        let monitors = monitors::enumerate_monitors();
        let mut initial_brightness = Vec::new();

        for m in &monitors {
            if let Some(b) = brightness::get_brightness(m) {
                initial_brightness.push(b);
            } else {
                initial_brightness.push(50); // Default if read fails
            }
        }

        APP_STATE = Some(AppState {
            monitors,
            brightness: initial_brightness,
            hot_monitor: None,
        });

        RENDERER = Some(render::Renderer::new()?);

        let instance = GetModuleHandleW(None)?.into();
        let _hwnd = window::create(instance, "Brightness Control", Some(wnd_proc))?;

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    Ok(())
}

extern "system" fn wnd_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match message {
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
                let x = (lparam.0 & 0xffff) as i32;
                let y = ((lparam.0 >> 16) & 0xffff) as i32;
                handle_input(window, x, y, true);
                SetCapture(window);
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                if (wparam.0 & MK_LBUTTON.0 as usize) != 0 {
                    let x = (lparam.0 & 0xffff) as i32;
                    let y = ((lparam.0 >> 16) & 0xffff) as i32;
                    handle_input(window, x, y, false);
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                DRAGGING_MONITOR_IDX = None;
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
        
        let slider_width = width - 2.0 * margin_x;

        // If clicking down, find which slider
        if is_down {
            for (i, _) in state.monitors.iter().enumerate() {
                // Monitor label is 16px. Slider is y + 24.
                // Slider track is at cur_y + 24 + 8 = cur_y + 32
                let slider_top = cur_y + 24.0 + 8.0;
                let slider_bottom = slider_top + 4.0;
                
                let hit_top = slider_top - 12.0;
                let hit_bottom = slider_bottom + 12.0;
                
                if (y as f32) >= hit_top && (y as f32) <= hit_bottom {
                    DRAGGING_MONITOR_IDX = Some(i);
                    break;
                }
                
                cur_y += 24.0 + monitor_spacing;
            }
        }

        // Processing movement/drag
        if let Some(idx) = DRAGGING_MONITOR_IDX {
            let pct = ((x as f32 - margin_x) / slider_width).clamp(0.0, 1.0);
            let new_val = (pct * 100.0) as u32;

            if state.brightness[idx] != new_val {
                state.brightness[idx] = new_val;
                
                // Hardware update
                let m = &state.monitors[idx];
                brightness::set_brightness(m, new_val);
                
                // Repaint
                let _ = InvalidateRect(Some(window), None, false);
            }
        }
    }
}
