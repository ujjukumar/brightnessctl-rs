use windows::core::Result;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};

pub unsafe fn init_com() -> Result<()> {
    CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()
}

pub unsafe fn init_dpi() -> Result<()> {
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
}
