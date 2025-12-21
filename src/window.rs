use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Gdi::*,
    Win32::UI::WindowsAndMessaging::*,
};

pub unsafe fn create(
    instance: HINSTANCE, 
    title: &str, 
    wnd_proc: WNDPROC,
    menu: Option<HMENU>,
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
        hbrBackground: CreateSolidBrush(COLORREF(0x1a1a1a)), 
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

    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class_name,
        &HSTRING::from(title),
        WS_OVERLAPPEDWINDOW | WS_VISIBLE,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        400,
        600,
        None,
        menu,
        Some(instance),
        None,
    )?;

    if hwnd.0.is_null() {
        return Err(Error::from_thread());
    }

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
    fn test_window_creation() {
        unsafe {
            let instance: HINSTANCE = GetModuleHandleW(None).unwrap().into();
            let result = create(instance, "Test Window", Some(test_wndproc), None);
            assert!(result.is_ok(), "Window creation failed: {:?}", result.err());
            let hwnd = result.unwrap();
            assert!(!hwnd.0.is_null());
            
            // Cleanup
            let _ = DestroyWindow(hwnd);
        }
    }
}