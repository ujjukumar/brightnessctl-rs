use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE},
    Win32::Graphics::Gdi::*,
    Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow},
    Win32::UI::WindowsAndMessaging::*,
};

/// Default client area size in DIPs; scaled to the window's DPI at creation.
const CLIENT_WIDTH: i32 = 384;
const CLIENT_HEIGHT: i32 = 544;

fn scale_for_dpi(value: i32, dpi: u32) -> i32 {
    (value * dpi as i32 + 48) / 96
}

/// Draws the system title bar dark or light. The menu bar stays light either way.
/// Fails harmlessly on Windows versions without the attribute.
pub fn set_dark_title_bar(hwnd: HWND, dark: bool) {
    let value = BOOL::from(dark);
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &value as *const BOOL as *const std::ffi::c_void,
            std::mem::size_of::<BOOL>() as u32,
        );
    }
}

/// `param` reaches `wnd_proc` in `WM_NCCREATE` (`CREATESTRUCTW::lpCreateParams`).
pub unsafe fn create(
    instance: HINSTANCE, 
    title: &str, 
    wnd_proc: WNDPROC,
    menu: Option<HMENU>,
    dark: bool,
    param: Option<*const std::ffi::c_void>,
) -> Result<HWND> {
    let class_name = w!("BrightnessCtlWindow");
    
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: wnd_proc,
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: instance,
        hIcon: LoadIconW(None, IDI_APPLICATION)?,
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        // No background brush: Direct2D paints every pixel, and erasing first flickers.
        hbrBackground: HBRUSH::default(),
        lpszMenuName: PCWSTR::null(),
        lpszClassName: class_name,
        hIconSm: HICON::default(),
    };

    let atom = RegisterClassExW(&wc);
    if atom == 0 {
        let err = GetLastError();
        if err != ERROR_CLASS_ALREADY_EXISTS {
             return Err(Error::from_hresult(HRESULT::from_win32(err.0)));
        }
    }

    // Created hidden at a default size: the DPI, and so the right size, is only
    // known once the window exists on a monitor.
    let style = WS_OVERLAPPEDWINDOW;
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class_name,
        &HSTRING::from(title),
        style,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        None,
        menu,
        Some(instance),
        param,
    )?;

    if hwnd.0.is_null() {
        return Err(Error::from_thread());
    }

    let dpi = GetDpiForWindow(hwnd);
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: scale_for_dpi(CLIENT_WIDTH, dpi),
        bottom: scale_for_dpi(CLIENT_HEIGHT, dpi),
    };
    let _ = AdjustWindowRectExForDpi(&mut rect, style, menu.is_some(), WINDOW_EX_STYLE::default(), dpi);
    let _ = SetWindowPos(
        hwnd,
        None,
        0,
        0,
        rect.right - rect.left,
        rect.bottom - rect.top,
        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
    );
    // Before showing, so a dark window never flashes a light title bar.
    set_dark_title_bar(hwnd, dark);
    let _ = ShowWindow(hwnd, SW_SHOW);

    Ok(hwnd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;

    extern "system" fn test_wndproc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        unsafe { DefWindowProcW(window, message, wparam, lparam) }
    }

    #[test]
    fn test_scale_for_dpi() {
        assert_eq!(scale_for_dpi(384, 96), 384);
        assert_eq!(scale_for_dpi(384, 120), 480);
        assert_eq!(scale_for_dpi(384, 144), 576);
        assert_eq!(scale_for_dpi(544, 168), 952);
    }

    #[test]
    fn test_window_creation() {
        unsafe {
            let instance: HINSTANCE = GetModuleHandleW(None).unwrap().into();
            let result = create(instance, "Test Window", Some(test_wndproc), None, false, None);
            assert!(result.is_ok(), "Window creation failed: {:?}", result.err());
            let hwnd = result.unwrap();
            assert!(!hwnd.0.is_null());
            
            // Cleanup
            let _ = DestroyWindow(hwnd);
        }
    }
}